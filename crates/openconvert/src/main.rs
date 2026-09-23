#![forbid(unsafe_code)]
//! The OpenConvert CLI: the engine's face (D13.1, IMPLEMENTATION_PLAN §2).
//!
//! Three channels, kept strictly apart (D13.2): stdout is data, stderr is NDJSON events,
//! and the exit code is the verdict. A supervisor never parses stderr text to find out what
//! happened.

mod cli;
mod cmd_convert;
mod cmd_diff_stage;
mod cmd_dump_stage;
mod cmd_inspect;
mod cmd_model;
mod cmd_validate;
mod control;

use std::io::Write;
use std::process::ExitCode as ProcessExitCode;

use oc_core::events::EventSink;
use oc_core::exit::ExitCode;

use crate::cli::{Command, Progress};

fn main() -> ProcessExitCode {
    let code = run();
    ProcessExitCode::from(u8::try_from(code.code()).unwrap_or(1))
}

fn run() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `--progress json` has to be known before parsing can fail, or a usage error would have
    // no channel to report itself on.
    let wants_events = args
        .windows(2)
        .any(|w| w[0] == "--progress" && w[1] == "json");

    let stderr = std::io::stderr();
    let mut events = EventSink::new(stderr.lock(), wants_events);

    match cli::parse(args) {
        Ok(Command::Print(text)) => {
            print!("{text}");
            ExitCode::Ok
        }
        Ok(Command::Convert(convert)) => {
            let mut events =
                EventSink::new(std::io::stderr().lock(), convert.progress == Progress::Json);
            cmd_convert::run(&convert, &mut events)
        }
        Ok(Command::Validate(validate)) => {
            let mut events = EventSink::new(
                std::io::stderr().lock(),
                validate.progress == Progress::Json,
            );
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            let code = cmd_validate::run(&validate, &mut events, &mut stdout);
            let _ = stdout.flush();
            code
        }
        Ok(Command::Inspect(inspect)) => {
            let mut events =
                EventSink::new(std::io::stderr().lock(), inspect.progress == Progress::Json);
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            let code = cmd_inspect::run(&inspect, &mut events, &mut stdout);
            let _ = stdout.flush();
            code
        }
        Ok(Command::DiffStage(diff)) => {
            let mut events =
                EventSink::new(std::io::stderr().lock(), diff.progress == Progress::Json);
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            let code = cmd_diff_stage::run(&diff, &mut events, &mut stdout);
            let _ = stdout.flush();
            code
        }
        Ok(Command::Model(model)) => {
            let mut events =
                EventSink::new(std::io::stderr().lock(), model.progress == Progress::Json);
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            let code = cmd_model::run(&model, &mut events, &mut stdout);
            let _ = stdout.flush();
            code
        }
        Ok(Command::DumpStage(dump)) => {
            let mut events =
                EventSink::new(std::io::stderr().lock(), dump.progress == Progress::Json);
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            let code = cmd_dump_stage::run(&dump, &mut events, &mut stdout);
            let _ = stdout.flush();
            code
        }
        Err(error) => {
            events.fatal("E_USAGE", &error.to_string());
            eprintln!("error: {error}\n\n{}", cli::USAGE);
            ExitCode::Usage
        }
    }
}
