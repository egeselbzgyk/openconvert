//! Running `tesseract` (PHASE 13 detail 2): a fixed argument vector, never a shell string, and a
//! deadline that is a hang detector.
//!
//! **Argv, not a command line.** Every argument is passed to the operating system as its own
//! string. Nothing is ever joined into a line a shell would parse, so a file name or a language spec
//! cannot become a second command; the language spec is additionally restricted to plain
//! traineddata names before it gets here ([`crate::ocr::lang::LangSpec::parse`]).
//!
//! **Owned like every other child.** The process is registered with [`super::supervise`] the moment
//! it exists, so the panic hook and the signal handler tear it down with the engine, and it is
//! spawned into the engine's own process group, so a supervisor's group kill reaches it too. A
//! `tesseract` that hangs is killed at the deadline rather than waited on.
//!
//! **`--oem` is never passed.** The LSTM default is what the accuracy evidence is about, and a
//! different engine mode is a different engine.

use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use super::supervise;
use crate::ocr::lang::LangSpec;
use crate::ocr::Psm;

/// Windows' `CREATE_NO_WINDOW`: without it a console flashes on every spawn (ARCHITECTURE §8.2).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Tesseract's output base name for "write to standard output".
const STDOUT_BASE: &str = "stdout";

/// The config file that selects TSV output.
const TSV_CONFIG: &str = "tsv";

/// Keep runs of spaces between words as Tesseract measured them. The pipeline joins words with
/// one space itself (detail 7), so this changes nothing in the text; it is on the command line
/// because the plan fixes the command line, and a fixed command line is what makes the argv spy a
/// complete description of what the engine asks for.
const PRESERVE_SPACES: &str = "preserve_interword_spaces=1";

/// OpenMP's thread cap for the child, and its value: one thread per `tesseract`.
///
/// Tesseract's LSTM parallelises with OpenMP, whose idle workers spin. On a machine that is already
/// busy — which a converting machine is — four spinning workers per call turned a one-second page
/// into a thirty-second one and every page hit `ocr.region_deadline_secs` (measured here at load
/// 9 on 4 cores: 0.96 s standalone, killed at 30 s inside a loaded run). Tesseract's own
/// documentation recommends the cap when it is not alone on the machine. An environment variable,
/// not an argument: the argument vector stays the fixed one detail 2 names.
pub const OMP_THREAD_LIMIT: (&str, &str) = ("OMP_THREAD_LIMIT", "1");

/// What one OCR call asks Tesseract for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OcrArgs {
    pub langs: LangSpec,
    pub psm: Psm,
    pub dpi: u32,
}

/// The command that reads `image` with `args`, printing TSV to stdout:
/// `tesseract <img> stdout -l <langs> --psm <n> --dpi <dpi> -c preserve_interword_spaces=1 tsv`.
pub fn command(program: &Path, image: &Path, args: &OcrArgs) -> Command {
    let mut command = Command::new(program);
    command
        .arg(image)
        .arg(STDOUT_BASE)
        .arg("-l")
        .arg(args.langs.arg())
        .arg("--psm")
        .arg(args.psm.number().to_string())
        .arg("--dpi")
        .arg(args.dpi.to_string())
        .arg("-c")
        .arg(PRESERVE_SPACES)
        .arg(TSV_CONFIG)
        .env(OMP_THREAD_LIMIT.0, OMP_THREAD_LIMIT.1);
    command
}

/// What a finished child printed, and how it ended.
#[derive(Clone, Debug)]
pub struct Captured {
    pub status: Option<ExitStatus>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Captured {
    /// Whether the process exited with status zero.
    pub fn success(&self) -> bool {
        self.status.is_some_and(|status| status.success())
    }
}

/// Why a child could not be run to completion.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    #[error("could not start {program}: {message}")]
    Spawn { program: String, message: String },
    /// Still running at the deadline, and killed.
    #[error("still running after {0:?}, and killed")]
    Deadline(Duration),
    #[error("could not read the output of {program}: {message}")]
    Io { program: String, message: String },
}

/// Run `command` to completion, capturing stdout and stderr, and kill it if it has not finished
/// writing its output within `deadline`.
pub fn run_captured(mut command: Command, deadline: Duration) -> Result<Captured, RunError> {
    let program = command.get_program().to_string_lossy().into_owned();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    supervise::install();
    let mut child = command.spawn().map_err(|error| RunError::Spawn {
        program: program.clone(),
        message: error.to_string(),
    })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let pid = supervise::register(child);

    // Both pipes are drained on their own threads: a child that fills the pipe nobody is reading
    // blocks forever, and that would look exactly like a hang.
    let (sender, receiver) = mpsc::channel();
    if let Some(mut out) = stdout {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let read = out.read_to_end(&mut bytes).map(|_| bytes);
            let _ = sender.send(read);
        });
    }
    let errors = stderr.map(|mut err| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = err.read_to_end(&mut bytes);
            bytes
        })
    });

    let stdout = match receiver.recv_timeout(deadline) {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(error)) => {
            supervise::kill(pid);
            return Err(RunError::Io {
                program,
                message: error.to_string(),
            });
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            supervise::kill(pid);
            return Err(RunError::Deadline(deadline));
        }
        // No stdout pipe at all cannot happen with `Stdio::piped`, and is treated as empty output.
        Err(mpsc::RecvTimeoutError::Disconnected) => Vec::new(),
    };
    let status = supervise::wait(pid);
    let stderr = errors
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    Ok(Captured {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every call runs single-threaded under OpenMP, so a busy machine slows it rather than stalling
    /// it past its deadline.
    #[test]
    fn every_call_caps_openmp_at_one_thread() {
        let args = OcrArgs {
            langs: LangSpec::single("eng"),
            psm: Psm::AutoOsd,
            dpi: 300,
        };
        let built = command(
            Path::new("/usr/bin/tesseract"),
            Path::new("page.png"),
            &args,
        );
        let envs: Vec<_> = built.get_envs().collect();
        assert!(
            envs.contains(&(
                std::ffi::OsStr::new(OMP_THREAD_LIMIT.0),
                Some(std::ffi::OsStr::new(OMP_THREAD_LIMIT.1))
            )),
            "{envs:?}"
        );
    }
}
