//! Running the engine: one argument, a validated job spec, in a directory the app controls.
//!
//! The conversion happens in the `openconvert` sidecar, never in this process (D13.1). This
//! module is the whole of how the app starts one:
//!
//! 1. **Validate** the job spec with the engine's own validator (`oc_core::jobspec`), before
//!    anything is written or spawned (test 12.2). An invalid spec never reaches a process.
//! 2. **Write** it into the app's job directory ([`crate::fs_scope::AppDirs::jobs`]).
//! 3. **Spawn** the engine with **exactly one argument**, that path (RT B15, test 12.1). The
//!    webview never spawns anything: the command is built here, in Rust, and nothing the UI
//!    sends can add an argument to it.
//!
//! The engine's stderr is NDJSON events, forwarded line by line to whoever started the job; the
//! UI parses them (`ui/src/lib/events.ts`), because it is the UI that must hard-error on a
//! protocol it does not speak. stdin is the control channel: [`Running::cancel`] writes
//! `{"t":"cancel"}` on it (D13.2).
//!
//! **An engine is a process tree** ([`crate::tree`]): a Unix engine leads a process group of its
//! own and a Windows engine is put in a job object of its own, so a kill ends whatever the engine
//! started along with it, an engine that crashes leaves nothing behind, and the app's exit ends
//! every tree still running (D13.2, ARCHITECTURE §8.2).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread::JoinHandle;

use oc_core::jobspec::JobSpec;
use serde::Serialize;

use crate::fs_scope::is_inside;
use crate::tree::{self, Tree};

/// What can go wrong between the UI asking for something and the engine running.
///
/// Serialised to the webview as `{kind, detail}`: the UI chooses the sentence from `kind`, in the
/// user's language, and never shows `detail` as the message (R10 §6.20).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum UiError {
    #[error("the job spec is invalid: {0}")]
    InvalidJobSpec(String),
    #[error("{0} is not inside the app's job directory")]
    OutsideJobsDir(String),
    #[error("{0}")]
    Io(String),
    /// The engine did not start, or did not say `hello` when asked for its version.
    #[error("the converter did not answer: {0}")]
    NoHello(String),
    /// A staged engine from another build (RT A5.7).
    #[error(
        "the bundled converter is version {engine} and this app is {app}; rebuild and re-stage \
         it (`cargo run -p xtask -- stage-sidecars`)"
    )]
    StaleEngine { engine: String, app: String },
    #[error("the converter speaks protocol {engine}; this app speaks {app}")]
    ProtocolMismatch { engine: u64, app: u64 },
    #[error("the converter writes IR version {engine}; this app reads {app}")]
    IrMismatch { engine: u64, app: u64 },
    #[error("no job {0}")]
    UnknownJob(String),
    /// A job that has not finished cannot be started again.
    #[error("job {0} has not finished")]
    JobBusy(String),
    /// The registry this build ships cannot be downloaded from: its pins are not filled in.
    #[error("models are not available in this build: {0}")]
    ModelsUnavailable(String),
    /// A model or pack the registry does not name.
    #[error("no model or pack {0} in the registry")]
    UnknownModel(String),
    /// A pack the registry names but this version does not offer (the validation pack in 1.0).
    #[error("{id} is not offered in this version: {reason}")]
    NotOffered { id: String, reason: String },
    /// A download before its licence was shown and accepted (UI_UX §2.4).
    #[error("the licence of {0} has not been accepted")]
    LicenseNotAccepted(String),
    /// Deleting, or downloading again, a model that is downloading.
    #[error("{0} is downloading")]
    ModelBusy(String),
    /// AI assistance would send document text to `host`, which is not this computer, and no
    /// consent names it (D10). Nothing was started; the UI asks again.
    #[error("sending document text to {host} needs your consent")]
    ConsentRequired { host: String },
    /// The engine could not be asked something outside a conversion (`openconvert provider …`).
    #[error("the converter could not answer: {0}")]
    Provider(String),
}

impl From<std::io::Error> for UiError {
    fn from(error: std::io::Error) -> Self {
        UiError::Io(error.to_string())
    }
}

/// Receives each line the engine writes on stderr.
pub type LineSink = Box<dyn FnMut(String) + Send + 'static>;

/// Something that can start the engine on a spec. [`ProcessLauncher`] in the app; a recording
/// double in the tests that must prove a spawn did *not* happen.
pub trait Launch: Send + Sync {
    /// Start the engine on `spec`. `password`, when given, is for this one run: it reaches the
    /// engine in its environment (`OC_PDF_PASSWORD`, D13.11) — never in the spec, never on the
    /// command line, never on disk.
    fn launch(
        &self,
        spec: &Path,
        password: Option<&str>,
        on_line: LineSink,
    ) -> Result<Box<dyn Running>, UiError>;
}

/// The variable a password reaches the engine in (D13.11).
pub const PASSWORD_VAR: &str = "OC_PDF_PASSWORD";

/// One running engine, as its supervisor sees it.
pub trait Running: Send {
    /// Ask it to stop: `{"t":"cancel"}` on stdin (D13.2).
    fn cancel(&mut self) -> Result<(), UiError>;
    /// Stop it now. The deadline-passed fallback, never the first resort.
    fn kill(&mut self) -> Result<(), UiError>;
    /// `Some(code)` once it has exited — `None` inside when a signal ended it — and `None` while
    /// it still runs.
    fn try_wait(&mut self) -> Result<Option<Option<i32>>, UiError>;
}

/// Starts conversions: validate, write, launch — in that order, always.
pub struct Engine<L: Launch> {
    jobs_dir: PathBuf,
    launcher: L,
}

impl<L: Launch> Engine<L> {
    pub fn new(jobs_dir: PathBuf, launcher: L) -> Self {
        Self { jobs_dir, launcher }
    }

    /// Start `spec` as job `job_id`.
    ///
    /// The spec is validated first, by the same function the engine runs on it. A spec that fails
    /// is never written and never launched: the error is returned and nothing else happens.
    pub fn start(
        &self,
        job_id: &str,
        spec: &JobSpec,
        password: Option<&str>,
        on_line: LineSink,
    ) -> Result<Box<dyn Running>, UiError> {
        spec.validate()
            .map_err(|error| UiError::InvalidJobSpec(error.to_string()))?;
        let path = write_spec(&self.jobs_dir, job_id, spec)?;
        self.launcher.launch(&path, password, on_line)
    }
}

/// Write a validated spec to `<jobs_dir>/<job_id>.json`, atomically.
fn write_spec(jobs_dir: &Path, job_id: &str, spec: &JobSpec) -> Result<PathBuf, UiError> {
    // The id becomes a file name; the schema's pattern (checked by `validate` when the spec
    // carries it) is the same one applied here, so no id can name a path.
    let safe = !job_id.is_empty()
        && job_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !safe {
        return Err(UiError::InvalidJobSpec(format!("job id {job_id:?}")));
    }
    let path = jobs_dir.join(format!("{job_id}.json"));
    let partial = jobs_dir.join(format!("{job_id}.json.part"));
    std::fs::write(&partial, spec.to_json())?;
    std::fs::rename(&partial, &path)?;
    Ok(path)
}

/// Launches the real engine binary.
pub struct ProcessLauncher {
    program: PathBuf,
    jobs_dir: PathBuf,
    /// The app's cache directory, named to every engine as `OC_CACHE_DIR`: where a full run saves
    /// what `structure` settled and a "Fix and rebuild" resumes from it (A12.4b).
    cache_dir: Option<PathBuf>,
}

/// The environment variable the engine reads its cache directory from.
pub const CACHE_VAR: &str = "OC_CACHE_DIR";

/// Win32's `CREATE_NO_WINDOW` process-creation flag, from `winbase.h`: without it a console window
/// flashes on every conversion (D13.2). An operating-system constant, not a tunable.
#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

impl ProcessLauncher {
    pub fn new(program: PathBuf, jobs_dir: PathBuf) -> Self {
        Self {
            program,
            jobs_dir,
            cache_dir: None,
        }
    }

    /// Name `dir` to every engine this starts as its cache directory.
    pub fn with_cache(mut self, dir: PathBuf) -> Self {
        self.cache_dir = Some(dir);
        self
    }

    /// The command that runs `spec`: the engine and **one argument**, the spec's path, which must
    /// be inside the job directory (RT B15).
    pub fn command(&self, spec: &Path, password: Option<&str>) -> Result<Command, UiError> {
        if !is_inside(&self.jobs_dir, spec) {
            return Err(UiError::OutsideJobsDir(spec.display().to_string()));
        }
        let mut command = Command::new(&self.program);
        command
            .arg(spec)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        // Only the job the user unlocked gets a password; every other job gets none, not even one
        // the app itself happened to inherit.
        match password {
            Some(password) => command.env(PASSWORD_VAR, password),
            None => command.env_remove(PASSWORD_VAR),
        };
        match &self.cache_dir {
            Some(dir) => command.env(CACHE_VAR, dir),
            None => command.env_remove(CACHE_VAR),
        };
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // Its own process group, so the whole group can be ended at once (D13.2).
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        Ok(command)
    }
}

impl Launch for ProcessLauncher {
    fn launch(
        &self,
        spec: &Path,
        password: Option<&str>,
        on_line: LineSink,
    ) -> Result<Box<dyn Running>, UiError> {
        let mut child = self.command(spec, password)?.spawn()?;
        let tree = tree::adopt(&child);
        let stdin = child.stdin.take();
        let reader = child
            .stderr
            .take()
            .map(|stderr| forward_lines(stderr, on_line));
        Ok(Box::new(Process {
            child,
            stdin,
            reader,
            tree,
        }))
    }
}

/// Forward every line of `stream` to `on_line` on a thread of its own, until the stream ends.
fn forward_lines(
    stream: impl std::io::Read + Send + 'static,
    mut on_line: LineSink,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            on_line(line);
        }
    })
}

/// A spawned engine.
struct Process {
    child: Child,
    stdin: Option<ChildStdin>,
    reader: Option<JoinHandle<()>>,
    /// Everything the engine started, ended with it. `None` only where the system could not make
    /// one (a Windows job object that could not be created): then the engine alone is killed.
    tree: Option<Tree>,
}

impl Running for Process {
    fn cancel(&mut self) -> Result<(), UiError> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Ok(());
        };
        writeln!(stdin, r#"{{"t":"cancel"}}"#)?;
        stdin.flush()?;
        Ok(())
    }

    fn kill(&mut self) -> Result<(), UiError> {
        if let Some(tree) = self.tree.as_mut() {
            tree.end();
        }
        match self.child.kill() {
            Ok(()) => Ok(()),
            // Already gone: the kill's purpose is met.
            Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn try_wait(&mut self) -> Result<Option<Option<i32>>, UiError> {
        let exited = match self.tree.as_mut() {
            Some(tree) => tree.try_wait(&mut self.child)?,
            None => self.child.try_wait()?,
        };
        match exited {
            Some(status) => {
                // The stderr reader ends when the pipe closes, which the exit guarantees; joining
                // it here means every event line has been forwarded before the exit is reported.
                if let Some(reader) = self.reader.take() {
                    let _ = reader.join();
                }
                Ok(Some(status.code()))
            }
            None => Ok(None),
        }
    }
}

impl Drop for Process {
    /// An engine is never left running behind a dropped handle — a job removed mid-run, the queue
    /// torn down. `Drop` alone is not enough supervision (panic-abort skips it; D13.2): the app's
    /// exit and `supervise`'s hooks end every tree through [`tree::end_all`] as well.
    fn drop(&mut self) {
        if matches!(self.try_wait(), Ok(Some(_))) {
            return;
        }
        if let Some(tree) = self.tree.as_mut() {
            tree.end();
            tree.release();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// What the engine said about itself (§2.3 `hello`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Hello {
    pub engine_version: String,
    pub ir_version: u64,
    pub protocol: u64,
    pub pdfium_version: String,
}

/// Ask the engine for its `hello` and refuse one this app was not built with (RT A5.7).
///
/// One argument here too: `--version`. The staged sidecar is the `externalBin` footgun — `tauri
/// dev` will happily run last week's engine — and a mismatched pair fails much later, in ways that
/// look like conversion bugs. So the app checks at startup and refuses to start instead.
pub fn handshake(program: &Path, app_version: &str) -> Result<Hello, UiError> {
    let mut command = Command::new(program);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|error| UiError::NoHello(format!("{}: {error}", program.display())))?;
    check_hello(&String::from_utf8_lossy(&output.stderr), app_version)
}

/// Judge the first event of `stderr` against this app's versions.
pub fn check_hello(stderr: &str, app_version: &str) -> Result<Hello, UiError> {
    let first = stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| UiError::NoHello("no events".to_owned()))?;
    let event: serde_json::Value = serde_json::from_str(first)
        .map_err(|_| UiError::NoHello(format!("not an event: {first}")))?;
    if event["t"] != "hello" {
        return Err(UiError::NoHello(format!(
            "the first event was {}",
            event["t"]
        )));
    }

    let app_protocol = u64::from(oc_core::events::PROTOCOL_VERSION);
    let protocol = event["protocol"].as_u64().unwrap_or_default();
    if protocol != app_protocol || event["v"].as_u64() != Some(app_protocol) {
        return Err(UiError::ProtocolMismatch {
            engine: protocol,
            app: app_protocol,
        });
    }
    let app_ir = u64::from(oc_model::IR_VERSION);
    let ir_version = event["ir_version"].as_u64().unwrap_or_default();
    if ir_version != app_ir {
        return Err(UiError::IrMismatch {
            engine: ir_version,
            app: app_ir,
        });
    }
    let engine_version = event["engine_version"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if engine_version != app_version {
        return Err(UiError::StaleEngine {
            engine: engine_version,
            app: app_version.to_owned(),
        });
    }
    Ok(Hello {
        engine_version,
        ir_version,
        protocol,
        pdfium_version: event["pdfium_version"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
    })
}

/// The engine's file name inside the app: the `openconvert` binary, staged by
/// `cargo run -p xtask -- stage-sidecars` under a name of its own (PHASE 15).
///
/// Not `openconvert`: `tauri-build` copies every `externalBin` into `target/<profile>/`, and a
/// sidecar of the engine's own name replaced the engine cargo had built there with whichever
/// build was last staged — so the workspace tests ran a stale engine and cargo, its fingerprint
/// fresh, never noticed.
pub const SIDECAR_NAME: &str = "openconvert-engine";

/// Where the sidecar is: beside this executable, as Tauri's `externalBin` places it — in
/// `Contents/MacOS`, in `usr/bin` of the AppImage, in the install directory on Windows, and in a
/// workspace build in `target/<profile>/`, where `tauri-build` copies the staged engine.
pub fn sidecar_path() -> Result<PathBuf, UiError> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| UiError::Io("the app's own directory is unknown".to_owned()))?;
    Ok(dir.join(format!("{SIDECAR_NAME}{}", std::env::consts::EXE_SUFFIX)))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::fs_scope::AppDirs;

    fn scratch(name: &str) -> AppDirs {
        let dir =
            std::env::temp_dir().join(format!("oc-desktop-engine-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        AppDirs::under(&dir).expect("made")
    }

    /// The name the app looks for is the name Tauri bundles and copies (PHASE 15 carry-over): a
    /// rename on one side only would leave the app running whatever file was there before.
    #[test]
    fn the_sidecar_the_app_runs_is_the_one_tauri_bundles() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("json");
        let bundled: Vec<&str> = config["bundle"]["externalBin"]
            .as_array()
            .expect("externalBin")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        assert!(
            bundled.contains(&format!("bin/{SIDECAR_NAME}").as_str()),
            "{bundled:?}"
        );
        let path = sidecar_path().expect("a path");
        assert!(path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with(SIDECAR_NAME)));
    }

    /// Records every launch, and launches nothing.
    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<PathBuf>>>);

    impl Launch for Recorder {
        fn launch(
            &self,
            spec: &Path,
            _password: Option<&str>,
            _on_line: LineSink,
        ) -> Result<Box<dyn Running>, UiError> {
            self.0
                .lock()
                .expect("not poisoned")
                .push(spec.to_path_buf());
            Err(UiError::Io("the recorder launches nothing".to_owned()))
        }
    }

    /// 12.1 — RT B15. The command has one argument, and it is a path inside the job directory.
    #[test]
    fn engine_is_spawned_with_exactly_one_argument() {
        let dirs = scratch("one-arg");
        let spec_path = write_spec(
            &dirs.jobs,
            "job-1",
            &JobSpec::new("/in/book.pdf".into(), "/out/book.epub".into()),
        )
        .expect("written");

        let launcher =
            ProcessLauncher::new("/opt/openconvert/openconvert".into(), dirs.jobs.clone());
        let command = launcher.command(&spec_path, None).expect("a command");
        let args: Vec<PathBuf> = command.get_args().map(PathBuf::from).collect();
        assert_eq!(args.len(), 1, "exactly one argument: {args:?}");
        assert_eq!(args[0], spec_path);
        assert!(is_inside(&dirs.jobs, &args[0]), "inside the job directory");

        // And a spec anywhere else is not launched at all.
        let elsewhere = std::env::temp_dir().join("oc-desktop-elsewhere.json");
        assert!(matches!(
            launcher.command(&elsewhere, None),
            Err(UiError::OutsideJobsDir(_))
        ));
    }

    /// 12.2 — an invalid spec never reaches the launcher, and is never written.
    #[test]
    fn job_spec_is_validated_before_spawn() {
        let dirs = scratch("validated");
        let recorder = Recorder::default();
        let engine = Engine::new(dirs.jobs.clone(), recorder.clone());

        let mut remote = JobSpec::new("/in/book.pdf".into(), "/out/book.epub".into());
        remote.ai = Some(oc_core::jobspec::AiSpec {
            enabled: true,
            endpoint: Some("https://api.example.com/v1".to_owned()),
            ..Default::default()
        });
        let mut bad_id = JobSpec::new("/in/book.pdf".into(), "/out/book.epub".into());
        bad_id.job_id = Some("../../etc".to_owned());

        for invalid in [remote, bad_id] {
            let result = engine.start("job-1", &invalid, None, Box::new(|_| {}));
            assert!(
                matches!(result, Err(UiError::InvalidJobSpec(_))),
                "refused as invalid"
            );
        }
        assert!(
            recorder.0.lock().expect("not poisoned").is_empty(),
            "the launcher was never called"
        );
        assert_eq!(
            std::fs::read_dir(&dirs.jobs).expect("reads").count(),
            0,
            "nothing was written"
        );

        // The converse, so the test cannot pass by refusing everything.
        let valid = JobSpec::new("/in/book.pdf".into(), "/out/book.epub".into());
        let _ = engine.start("job-2", &valid, None, Box::new(|_| {}));
        assert_eq!(recorder.0.lock().expect("not poisoned").len(), 1);
    }

    /// D13.11 — a password the user typed reaches the engine in its environment for that one job,
    /// and nowhere else: not an argument, not the spec on disk.
    #[test]
    fn a_password_travels_in_the_environment_only() {
        let dirs = scratch("password");
        let spec = JobSpec::new("/in/locked.pdf".into(), "/out/locked.epub".into());
        let path = write_spec(&dirs.jobs, "job-1", &spec).expect("written");
        let launcher =
            ProcessLauncher::new("/opt/openconvert/openconvert".into(), dirs.jobs.clone());

        let unlocked = launcher
            .command(&path, Some("hunter22"))
            .expect("a command");
        assert_eq!(unlocked.get_args().count(), 1, "still exactly one argument");
        assert!(unlocked
            .get_envs()
            .any(|(key, value)| key == PASSWORD_VAR && value == Some("hunter22".as_ref())));
        assert!(!std::fs::read_to_string(&path)
            .expect("reads")
            .contains("hunter22"));

        let ordinary = launcher.command(&path, None).expect("a command");
        assert!(
            ordinary
                .get_envs()
                .any(|(key, value)| key == PASSWORD_VAR && value.is_none()),
            "an inherited password is removed from every other job"
        );
    }

    /// Every engine is told where the app's cache is, so a full run saves what `structure`
    /// settled and a "Fix and rebuild" resumes from it (A12.4b) — and an engine started without a
    /// cache directory gets none, not one inherited from wherever the app was launched.
    #[test]
    fn the_engine_is_told_where_the_cache_is() {
        let dirs = scratch("cache");
        let spec = JobSpec::new("/in/book.pdf".into(), "/out/book.epub".into());
        let path = write_spec(&dirs.jobs, "job-1", &spec).expect("written");

        let cached = ProcessLauncher::new("/opt/openconvert/openconvert".into(), dirs.jobs.clone())
            .with_cache(dirs.cache.clone())
            .command(&path, None)
            .expect("a command");
        assert_eq!(cached.get_args().count(), 1, "still exactly one argument");
        assert!(cached
            .get_envs()
            .any(|(key, value)| key == CACHE_VAR && value == Some(dirs.cache.as_os_str())));

        let uncached =
            ProcessLauncher::new("/opt/openconvert/openconvert".into(), dirs.jobs.clone())
                .command(&path, None)
                .expect("a command");
        assert!(uncached
            .get_envs()
            .any(|(key, value)| key == CACHE_VAR && value.is_none()));
    }

    #[test]
    fn a_job_id_cannot_name_a_path() {
        let dirs = scratch("ids");
        let spec = JobSpec::new("/in/book.pdf".into(), "/out/book.epub".into());
        for id in ["", "../x", "a/b", "a b"] {
            assert!(matches!(
                write_spec(&dirs.jobs, id, &spec),
                Err(UiError::InvalidJobSpec(_))
            ));
        }
    }

    fn hello_line(engine_version: &str, protocol: u64, ir: u64) -> String {
        serde_json::json!({
            "v": protocol, "t": "hello", "seq": 0, "ts_ms": 1,
            "engine_version": engine_version, "ir_version": ir, "protocol": protocol,
            "pdfium_version": "151.0.7881.0", "capabilities": ["inspect"]
        })
        .to_string()
    }

    #[test]
    fn the_handshake_refuses_every_mismatch_by_name() {
        let app = "0.1.0";
        assert!(check_hello(&hello_line(app, 1, 1), app).is_ok());
        assert_eq!(
            check_hello(&hello_line("0.0.9", 1, 1), app),
            Err(UiError::StaleEngine {
                engine: "0.0.9".to_owned(),
                app: app.to_owned()
            })
        );
        assert!(matches!(
            check_hello(&hello_line(app, 2, 1), app),
            Err(UiError::ProtocolMismatch { engine: 2, app: 1 })
        ));
        assert!(matches!(
            check_hello(&hello_line(app, 1, 2), app),
            Err(UiError::IrMismatch { engine: 2, app: 1 })
        ));
        assert!(matches!(check_hello("", app), Err(UiError::NoHello(_))));
        assert!(matches!(
            check_hello("openconvert 0.1.0", app),
            Err(UiError::NoHello(_))
        ));
    }
}
