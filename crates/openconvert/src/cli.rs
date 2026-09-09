//! Argument parsing for the engine's CLI (IMPLEMENTATION_PLAN §2.1).
//!
//! Hand-rolled rather than `clap`. Phase 0 implements one subcommand with three flags, and
//! §0.2 asks for the least code that does the job; a parser generator earns its dependency
//! when `convert` arrives with its twenty flags, and that is the phase to add it in.

use std::path::PathBuf;

/// How progress is reported (§2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Progress {
    /// Nothing on stderr.
    #[default]
    None,
    /// NDJSON events on stderr (§2.3).
    Json,
}

/// A parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Inspect(InspectArgs),
    /// `--help` or `--version`: print and exit successfully.
    Print(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectArgs {
    pub input: PathBuf,
    pub json: bool,
    pub pages: Vec<u32>,
    pub password: Option<String>,
    pub progress: Progress,
}

/// Why a command line was rejected. Every one of these is exit code 2 (§2.4).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CliError {
    #[error("no subcommand given")]
    NoSubcommand,
    #[error("unknown subcommand `{0}`")]
    UnknownSubcommand(String),
    #[error("unknown option `{0}`")]
    UnknownOption(String),
    #[error("`{0}` needs a value")]
    MissingValue(&'static str),
    #[error("`{value}` is not a valid {what}")]
    BadValue { what: &'static str, value: String },
    #[error("inspect needs exactly one input file")]
    InputCount,
}

pub const USAGE: &str = "\
openconvert — PDF to reflowable EPUB

usage:
  openconvert inspect <INPUT.pdf> [--json] [--pages <RANGE>] [--password <STRING>]
                                  [--progress none|json]
  openconvert --version
  openconvert --help

  --json               machine-readable report on stdout
  --pages <RANGE>      e.g. 1-10,20 (one-based, as printed)
  --password <STRING>  or the OC_PDF_PASSWORD environment variable
  --progress json      NDJSON events on stderr; stdout stays data only
";

/// Parse the arguments after the program name.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, CliError> {
    let mut args = args.into_iter().peekable();

    let first = args.next().ok_or(CliError::NoSubcommand)?;
    match first.as_str() {
        "--help" | "-h" => return Ok(Command::Print(USAGE.to_owned())),
        "--version" | "-V" => {
            return Ok(Command::Print(format!(
                "openconvert {}\n",
                env!("CARGO_PKG_VERSION")
            )))
        }
        "inspect" => {}
        other => return Err(CliError::UnknownSubcommand(other.to_owned())),
    }

    let mut inputs = Vec::new();
    let mut parsed = InspectArgs {
        input: PathBuf::new(),
        json: false,
        pages: Vec::new(),
        password: std::env::var("OC_PDF_PASSWORD").ok(),
        progress: Progress::None,
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => parsed.json = true,
            "--pages" => {
                let value = args.next().ok_or(CliError::MissingValue("--pages"))?;
                parsed.pages = parse_page_range(&value)?;
            }
            "--password" => {
                parsed.password = Some(args.next().ok_or(CliError::MissingValue("--password"))?);
            }
            "--progress" => {
                let value = args.next().ok_or(CliError::MissingValue("--progress"))?;
                parsed.progress = match value.as_str() {
                    "none" => Progress::None,
                    "json" => Progress::Json,
                    _ => {
                        return Err(CliError::BadValue {
                            what: "--progress value",
                            value,
                        })
                    }
                };
            }
            "--help" | "-h" => return Ok(Command::Print(USAGE.to_owned())),
            other if other.starts_with('-') => {
                return Err(CliError::UnknownOption(other.to_owned()))
            }
            other => inputs.push(PathBuf::from(other)),
        }
    }

    match inputs.len() {
        1 => {
            parsed.input = inputs.remove(0);
            Ok(Command::Inspect(parsed))
        }
        _ => Err(CliError::InputCount),
    }
}

/// Parse `1-10,20` into zero-based page indices.
///
/// The range is one-based on the command line because that is how a reader counts pages,
/// and zero-based inside because that is how the IR indexes them (D13.3). The conversion
/// happens exactly here so the two conventions never meet anywhere else.
fn parse_page_range(text: &str) -> Result<Vec<u32>, CliError> {
    let bad = |value: &str| CliError::BadValue {
        what: "page range",
        value: value.to_owned(),
    };
    let one_based = |s: &str| -> Result<u32, CliError> {
        let n: u32 = s.trim().parse().map_err(|_| bad(text))?;
        n.checked_sub(1).ok_or_else(|| bad(text))
    };

    let mut pages = Vec::new();
    for part in text.split(',').filter(|p| !p.trim().is_empty()) {
        match part.split_once('-') {
            Some((start, end)) => {
                let (start, end) = (one_based(start)?, one_based(end)?);
                if start > end {
                    return Err(bad(text));
                }
                pages.extend(start..=end);
            }
            None => pages.push(one_based(part)?),
        }
    }
    if pages.is_empty() {
        return Err(bad(text));
    }
    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}
