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
    /// Boxed: the conversion's flags outweigh every other command's several times over.
    Convert(Box<ConvertArgs>),
    Validate(ValidateArgs),
    Inspect(InspectArgs),
    DumpStage(DumpStageArgs),
    DiffStage(DiffStageArgs),
    /// One job-spec path and nothing else: how the desktop app runs a conversion (D13.2,
    /// RT B15). Everything the job needs is in the file.
    Job(PathBuf),
    Model(ModelArgs),
    Provider(ProviderArgs),
    /// `--help`: print and exit successfully.
    Print(String),
    /// `--version`: print the version, and answer a supervisor's version handshake (RT A5.7).
    Version(String),
}

/// `convert <INPUT.pdf>`: the whole pipeline, PDF to EPUB.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvertArgs {
    pub input: PathBuf,
    /// `<input>.epub` next to the input when absent.
    pub output: Option<PathBuf>,
    pub preset: oc_model::document::PresetName,
    /// Force `dc:language` and skip detection.
    pub language: Option<oc_model::lang::LangTag>,
    pub password: Option<String>,
    pub progress: Progress,
    /// `dcterms:modified`, forced.
    ///
    /// Not in the plan's flag list, and it earns its place: it is the one field that differs
    /// between two builds of the same book, so the cross-OS byte-identity gate (D13.8, test
    /// 5.5) has to be able to hold it still from the command line. Without it the gate would
    /// have to redact bytes out of a zip, which is not a thing a CI job should be doing.
    pub modified: Option<String>,
    /// Where `report.json` goes. `<output>.report.json` when absent (§2.1).
    pub report: Option<PathBuf>,
    /// Which locale the warnings are printed in.
    ///
    /// §2.1's flag list does not have it and §2.2's job spec does (`"locale"`, default `"en"`).
    /// Without a flag the engine's own localisation would be unreachable from the command line,
    /// which would make three template files that only a GUI could read — and the GUI is Phase 12.
    /// Precedence is CLI > job-spec either way (D13.11), so the flag is the spec's field named.
    pub locale: oc_core::warnings::Locale,
    /// The user's corrections (`overrides.json`, ARCHITECTURE §4.7).
    pub overrides: Option<PathBuf>,
    /// How the model is reached, when it is (PHASE 10). `None` is `ai.enabled = false`: the v1
    /// default, and what `--no-ai` forces whatever else the line says.
    pub ai: Option<AiArgs>,
    /// `--ocr`: whether scanned content is read (PHASE 13). Default `auto`, which follows the page
    /// class.
    pub ocr: oc_core::ocr::OcrMode,
    /// `--ocr-path`: an explicit `tesseract`, which replaces discovery rather than being tried
    /// first — a missing one is "no engine", not a reason to look elsewhere.
    pub ocr_path: Option<PathBuf>,
    /// `--ocr-lang`: traineddata names, `+`-joined. Default: from the document's language.
    pub ocr_lang: Option<oc_core::ocr::lang::LangSpec>,
    /// `--re-ocr`: whether an OCR sandwich's own layer is replaced. Default `never` (D13.10).
    pub re_ocr: oc_core::ocr::ReOcr,
    /// `--max-pages` and `--max-memory` (§2.1); every other limit is `thresholds.toml`'s.
    pub limits: oc_core::limits::Limits,
}

pub use openconvert::ai_endpoint::AiArgs;

/// `validate <INPUT.epub>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidateArgs {
    pub input: PathBuf,
    /// 1 = Tier 1 only, 2 = plus EPUBCheck.
    pub tier: u8,
    pub json: bool,
    pub epubcheck_jar: Option<PathBuf>,
    pub progress: Progress,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectArgs {
    pub input: PathBuf,
    pub json: bool,
    pub pages: Vec<u32>,
    pub password: Option<String>,
    pub progress: Progress,
    /// Refuse a document with more pages than this, before any page is read.
    ///
    /// The flag lands in Phase 1 rather than with the rest of the resource controls in
    /// Phase 14, because a limit that is added after the code it bounds is a limit with a
    /// window in it (Phase 1 detail 8). `None` means the shipped default.
    pub max_pages: Option<u32>,
}

/// `diff-stage <STAGE> <INPUT>`: what a stage did to the text, including when it did not
/// balance (PHASE 7.5).
///
/// The same arguments as `dump-stage`, and deliberately so: the two answer the two halves of
/// the same question and a reader who knows one should not have to learn the other.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffStageArgs {
    pub stage: String,
    pub input: PathBuf,
    pub password: Option<String>,
    pub progress: Progress,
    pub limits: oc_core::limits::Limits,
}

/// `dump-stage <STAGE> <INPUT>`: everything a stage produced, as canonical JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DumpStageArgs {
    /// One of the twelve stage names. Only `ingest` can be dumped so far, and the command
    /// says so rather than pretending the others produce nothing.
    pub stage: String,
    pub input: PathBuf,
    pub password: Option<String>,
    pub progress: Progress,
    pub limits: oc_core::limits::Limits,
}

/// What `model` is asked to do (§2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelAction {
    Pull,
    List,
    Remove,
}

/// `model <pull|list|remove> [ID]`: the model manager (PHASE 9 details 6–7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelArgs {
    pub action: ModelAction,
    /// Required by `pull` and `remove`, refused by `list`.
    pub id: Option<String>,
    /// The bundled `models.toml` when absent.
    pub registry: Option<PathBuf>,
    /// The per-OS data directory when absent.
    pub dir: Option<PathBuf>,
    pub json: bool,
    pub progress: Progress,
}

/// What `provider` is asked to do (PHASE 11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderAction {
    /// Is Ollama on `localhost:11434`, and what does it serve?
    Detect,
    /// Does this URL need consent, and would the engine use it? Nothing is sent.
    Check,
    /// What would `convert --ai` open at this URL? Asks the endpoint what it is; sends no question.
    Probe,
}

/// `provider detect | check <URL> | probe <URL>`: what the desktop's Provider settings read
/// (UI_UX §2.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderArgs {
    pub action: ProviderAction,
    /// For `probe`: the URL is `ai.endpoint`, and the other fields are the `convert --ai` flags
    /// of the same names, read the same way.
    pub ai: AiArgs,
    pub json: bool,
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
    #[error("dump-stage needs a stage name and exactly one input file")]
    DumpStageArgs,
    #[error("diff-stage needs a stage name and exactly one input file")]
    DiffStageArgs,
    #[error("convert needs exactly one input file")]
    ConvertArgs,
    #[error("validate needs exactly one input file")]
    ValidateArgs,
    #[error("model needs pull <ID>, list, or remove <ID>")]
    ModelArgs,
    #[error("`{0}` only means something with `--ai`")]
    NeedsAi(&'static str),
    #[error("provider needs detect, check <URL>, or probe <URL> [--llm-provider …] [--llm-model …] [--llm-allow-host …] [--llm-api-key-file …]")]
    ProviderArgs,
}

pub const USAGE: &str = "\
openconvert — PDF to reflowable EPUB

usage:
  openconvert convert <INPUT.pdf> [-o <OUT.epub>] [--preset <NAME>] [--lang <TAG>]
                                  [--password <STRING>] [--progress none|json]
                                  [--modified <YYYY-MM-DDThh:mm:ssZ>] [--report <PATH.json>]
                                  [--locale en|de|tr] [--overrides <PATH.json>]
                                  [--ai [--ai-all-tasks] [--ai-mode fast|quality]
                                        [--llm-endpoint <URL>]
                                        [--llm-provider builtin|ollama|openai-compatible]
                                        [--llm-model <NAME>] [--llm-allow-host <HOST>]
                                        [--llm-api-key-file <PATH>] [--model-path <PATH>]]
                                  [--no-ai]
                                  [--ocr auto|never|always] [--ocr-path <PATH>]
                                  [--ocr-lang <SPEC>] [--re-ocr never|auto|always]
                                  [--max-pages <N>] [--max-memory <BYTES|4GiB>]
  openconvert validate <INPUT.epub> [--tier 1|2] [--json] [--epubcheck-jar <PATH>]
  openconvert inspect <INPUT.pdf> [--json] [--pages <RANGE>] [--password <STRING>]
                                  [--progress none|json] [--max-pages <N>]
  openconvert dump-stage <STAGE> <INPUT.pdf> [--password <STRING>]
                                  [--progress none|json] [--max-pages <N>]
  openconvert diff-stage <STAGE> <INPUT.pdf> [--password <STRING>]
                                  [--progress none|json] [--max-pages <N>]
  openconvert model pull <ID> [--registry <PATH>] [--dir <PATH>] [--progress none|json]
  openconvert model list [--json] [--registry <PATH>] [--dir <PATH>]
  openconvert model remove <ID> [--dir <PATH>]
  openconvert provider detect [--json]
  openconvert provider check <URL> [--json]
  openconvert provider probe <URL> [--llm-provider <P>] [--llm-model <NAME>]
                                  [--llm-allow-host <HOST>] [--llm-api-key-file <PATH>] [--json]
  openconvert <JOB.json>
  openconvert --version
  openconvert --help

  <JOB.json>           a job spec (schemas/job-spec.v1.json) as the only argument: how the
                       desktop app runs a conversion. Events are always NDJSON on stderr.
                       Its `ai` object is --ai: endpoint, api_key_file, model_path and
                       model_id are the --llm-*/--model-path flags, and non_loopback_consent
                       is --llm-allow-host for the endpoint's own host.

  --json               machine-readable report on stdout
  --pages <RANGE>      e.g. 1-10,20 (one-based, as printed)
  --password <STRING>  or the OC_PDF_PASSWORD environment variable
  --progress json      NDJSON events on stderr; stdout stays data only
  --max-pages <N>      refuse a document with more pages than this
  --max-memory <SIZE>  convert: the engine's address-space cap, e.g. 4GiB (RLIMIT_AS on Unix)

  -o, --output <PATH>  where the EPUB goes; default <input>.epub beside the input
  --preset <NAME>      auto|novel|academic|textbook|poetry|scanned (default auto)
  --lang <TAG>         force dc:language and skip detection
  --modified <STAMP>   force dcterms:modified, for byte-identical output
  --report <PATH>      where report.json goes; default <output>.report.json
  --locale <TAG>       en|de|tr; which language the warnings are printed in (default en)
  --overrides <PATH>   the user's metadata and TOC corrections (overrides.json); with
                       OC_CACHE_DIR set, a run resumes after `structure` from the last
                       full run of the same PDF, which saves there
  --ocr <MODE>         auto|never|always; auto reads scanned pages and uncovered image
                       regions with the system Tesseract 5, when one is found (default auto)
  --ocr-path <PATH>    the tesseract binary to use instead of searching for one
  --ocr-lang <SPEC>    Tesseract language data, e.g. deu or deu+eng (default: the book's language)
  --re-ocr <MODE>      never|auto|always; replace an OCR sandwich's own text layer (default never)
  --ai                 ask a model the four once-per-book questions; never fails a conversion
  --llm-endpoint <URL> a server you run: llama-server, Ollama, LM Studio, any OpenAI-compatible
                       one; the engine asks it what it is before asking it anything else
  --llm-provider <P>   builtin (start the bundled server), ollama (localhost:11434 unless
                       --llm-endpoint says otherwise), openai-compatible
  --llm-model <NAME>   the model as the server names it; needed when it serves more than one
  --llm-allow-host <HOST>
                       consent to sending the book's text to HOST, which must be the endpoint's
                       own host; an endpoint off this computer is refused without it (https only)
  --llm-api-key-file <PATH>
                       a file holding the endpoint's key; there is no flag for the key itself
  --tier <1|2>         1 = the internal validator (default), 2 = plus EPUBCheck
  --registry <PATH>    a models.toml to use instead of the one built into the engine
  --dir <PATH>         the model store; default the per-OS data directory

  dump-stage writes one canonical-JSON object per line: a header, then one per page.
  <STAGE> is one of the twelve stage names; `ingest` and `text` are implemented so far.

  diff-stage runs <STAGE> outside the conservation check and reports what it did to the
  text: characters lost and gained, and which input blocks the book does not contain.
  Unlike dump-stage it still answers when the stage does not balance, which is when the
  answer is wanted. `structure` is implemented so far.
";

/// Parse the arguments after the program name.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, CliError> {
    let args: Vec<String> = args.into_iter().collect();
    // The GUI's form: exactly one argument, a job spec (D13.2). Recognised by being the only
    // argument and naming a `.json` file, so that a mistyped subcommand is still reported as
    // one rather than as a job spec that could not be read.
    if let [only] = args.as_slice() {
        if is_job_spec_path(only) {
            return Ok(Command::Job(PathBuf::from(only)));
        }
    }
    let mut args = args.into_iter().peekable();

    let first = args.next().ok_or(CliError::NoSubcommand)?;
    match first.as_str() {
        "--help" | "-h" => return Ok(Command::Print(USAGE.to_owned())),
        "--version" | "-V" => {
            return Ok(Command::Version(format!(
                "openconvert {}\n",
                env!("CARGO_PKG_VERSION")
            )))
        }
        "inspect" => {}
        "convert" => return parse_convert(args),
        "validate" => return parse_validate(args),
        "dump-stage" => return parse_dump_stage(args),
        "diff-stage" => return parse_diff_stage(args),
        "model" => return parse_model(args),
        "provider" => return parse_provider_command(args),
        other => return Err(CliError::UnknownSubcommand(other.to_owned())),
    }

    let mut inputs = Vec::new();
    let mut parsed = InspectArgs {
        input: PathBuf::new(),
        json: false,
        pages: Vec::new(),
        password: std::env::var("OC_PDF_PASSWORD").ok(),
        progress: Progress::None,
        max_pages: None,
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
            "--max-pages" => {
                let value = args.next().ok_or(CliError::MissingValue("--max-pages"))?;
                parsed.max_pages = Some(value.parse().map_err(|_| CliError::BadValue {
                    what: "--max-pages value",
                    value,
                })?);
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

/// Whether a lone argument names a job spec rather than a subcommand or a flag.
fn is_job_spec_path(arg: &str) -> bool {
    !arg.starts_with('-')
        && std::path::Path::new(arg)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

/// Parse `convert <INPUT.pdf>`, having already consumed the subcommand.
fn parse_convert<I: Iterator<Item = String>>(mut args: I) -> Result<Command, CliError> {
    let mut inputs = Vec::new();
    let mut parsed = ConvertArgs {
        input: PathBuf::new(),
        output: None,
        preset: oc_model::document::PresetName::Auto,
        language: None,
        password: std::env::var("OC_PDF_PASSWORD").ok(),
        progress: Progress::None,
        modified: None,
        report: None,
        locale: oc_core::warnings::Locale::En,
        overrides: None,
        ai: None,
        ocr: oc_core::ocr::OcrMode::Auto,
        ocr_path: None,
        ocr_lang: None,
        re_ocr: oc_core::ocr::ReOcr::Never,
        limits: oc_core::limits::Limits::default(),
    };
    let mut ai = false;
    let mut no_ai = false;
    let mut ai_args = AiArgs::default();
    // The first AI-only flag given, for the error when `--ai` is not.
    let mut ai_only: Option<&'static str> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--ocr" => {
                let value = args.next().ok_or(CliError::MissingValue("--ocr"))?;
                parsed.ocr = oc_core::ocr::OcrMode::parse(&value).ok_or(CliError::BadValue {
                    what: "--ocr value",
                    value,
                })?;
            }
            "--ocr-path" => {
                parsed.ocr_path = Some(PathBuf::from(
                    args.next().ok_or(CliError::MissingValue("--ocr-path"))?,
                ));
            }
            "--ocr-lang" => {
                let value = args.next().ok_or(CliError::MissingValue("--ocr-lang"))?;
                parsed.ocr_lang = Some(oc_core::ocr::lang::LangSpec::parse(&value).ok_or(
                    CliError::BadValue {
                        what: "--ocr-lang value",
                        value,
                    },
                )?);
            }
            "--max-pages" => {
                let value = args.next().ok_or(CliError::MissingValue("--max-pages"))?;
                parsed.limits.max_pages =
                    value
                        .parse()
                        .ok()
                        .filter(|pages| *pages > 0)
                        .ok_or(CliError::BadValue {
                            what: "--max-pages value",
                            value,
                        })?;
            }
            "--max-memory" => {
                let value = args.next().ok_or(CliError::MissingValue("--max-memory"))?;
                parsed.limits.max_memory_bytes = openconvert::sandbox::parse_bytes(&value)
                    .filter(|bytes| *bytes > 0)
                    .ok_or(CliError::BadValue {
                        what: "--max-memory value (bytes, or a binary unit such as 4GiB)",
                        value,
                    })?;
            }
            "--re-ocr" => {
                let value = args.next().ok_or(CliError::MissingValue("--re-ocr"))?;
                parsed.re_ocr = oc_core::ocr::ReOcr::parse(&value).ok_or(CliError::BadValue {
                    what: "--re-ocr value",
                    value,
                })?;
            }
            "-o" | "--output" => {
                parsed.output = Some(PathBuf::from(
                    args.next().ok_or(CliError::MissingValue("--output"))?,
                ));
            }
            "--preset" => {
                let value = args.next().ok_or(CliError::MissingValue("--preset"))?;
                parsed.preset = parse_preset(&value)?;
            }
            "--lang" => {
                let value = args.next().ok_or(CliError::MissingValue("--lang"))?;
                parsed.language = Some(oc_model::lang::LangTag::new(&value));
            }
            "--modified" => {
                parsed.modified = Some(args.next().ok_or(CliError::MissingValue("--modified"))?);
            }
            "--report" => {
                parsed.report = Some(PathBuf::from(
                    args.next().ok_or(CliError::MissingValue("--report"))?,
                ));
            }
            "--locale" => {
                let value = args.next().ok_or(CliError::MissingValue("--locale"))?;
                parsed.locale = oc_core::warnings::Locale::from_tag(&value);
            }
            "--overrides" => {
                parsed.overrides = Some(PathBuf::from(
                    args.next().ok_or(CliError::MissingValue("--overrides"))?,
                ));
            }
            "--password" => {
                parsed.password = Some(args.next().ok_or(CliError::MissingValue("--password"))?);
            }
            "--progress" => {
                let value = args.next().ok_or(CliError::MissingValue("--progress"))?;
                parsed.progress = parse_progress(&value)?;
            }
            // v1 ships with the LLM off, so `--no-ai` is the default spelled out. It is
            // accepted from day one because every script that wants determinism will write it,
            // and it wins over `--ai`: a script that asks for determinism gets it.
            "--no-ai" => no_ai = true,
            "--ai" => ai = true,
            "--ai-all-tasks" => {
                ai_args.all_tasks = true;
                ai_only.get_or_insert("--ai-all-tasks");
            }
            "--llm-endpoint" => {
                ai_args.endpoint = Some(
                    args.next()
                        .ok_or(CliError::MissingValue("--llm-endpoint"))?,
                );
                ai_only.get_or_insert("--llm-endpoint");
            }
            "--llm-api-key-file" => {
                ai_args.api_key_file = Some(PathBuf::from(
                    args.next()
                        .ok_or(CliError::MissingValue("--llm-api-key-file"))?,
                ));
                ai_only.get_or_insert("--llm-api-key-file");
            }
            "--model-path" => {
                ai_args.model_path = Some(PathBuf::from(
                    args.next().ok_or(CliError::MissingValue("--model-path"))?,
                ));
                ai_only.get_or_insert("--model-path");
            }
            "--llm-provider" => {
                let value = args
                    .next()
                    .ok_or(CliError::MissingValue("--llm-provider"))?;
                ai_args.provider = Some(parse_provider(&value)?);
                ai_only.get_or_insert("--llm-provider");
            }
            "--ai-mode" => {
                let value = args.next().ok_or(CliError::MissingValue("--ai-mode"))?;
                ai_args.mode = match value.as_str() {
                    "fast" => oc_core::jobspec::AiMode::Fast,
                    "quality" => oc_core::jobspec::AiMode::Quality,
                    _ => {
                        return Err(CliError::BadValue {
                            what: "--ai-mode value",
                            value,
                        })
                    }
                };
                ai_only.get_or_insert("--ai-mode");
            }
            "--llm-model" => {
                ai_args.model = Some(args.next().ok_or(CliError::MissingValue("--llm-model"))?);
                ai_only.get_or_insert("--llm-model");
            }
            "--llm-allow-host" => {
                ai_args.allow_host = Some(
                    args.next()
                        .ok_or(CliError::MissingValue("--llm-allow-host"))?,
                );
                ai_only.get_or_insert("--llm-allow-host");
            }
            "--help" | "-h" => return Ok(Command::Print(USAGE.to_owned())),
            other if other.starts_with('-') => {
                return Err(CliError::UnknownOption(other.to_owned()))
            }
            other => inputs.push(PathBuf::from(other)),
        }
    }

    // A flag that does nothing is worse than no flag: the AI-only ones need `--ai`.
    if let (false, Some(flag)) = (ai, ai_only) {
        return Err(CliError::NeedsAi(flag));
    }
    parsed.ai = (ai && !no_ai).then_some(ai_args);

    match inputs.len() {
        1 => {
            parsed.input = inputs.remove(0);
            Ok(Command::Convert(Box::new(parsed)))
        }
        _ => Err(CliError::ConvertArgs),
    }
}

/// Parse `validate <INPUT.epub>`, having already consumed the subcommand.
fn parse_validate<I: Iterator<Item = String>>(mut args: I) -> Result<Command, CliError> {
    let mut inputs = Vec::new();
    let mut parsed = ValidateArgs {
        input: PathBuf::new(),
        tier: 1,
        json: false,
        epubcheck_jar: None,
        progress: Progress::None,
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tier" => {
                let value = args.next().ok_or(CliError::MissingValue("--tier"))?;
                parsed.tier = match value.as_str() {
                    "1" => 1,
                    "2" => 2,
                    _ => {
                        return Err(CliError::BadValue {
                            what: "--tier value",
                            value,
                        })
                    }
                };
            }
            "--json" => parsed.json = true,
            "--epubcheck-jar" => {
                parsed.epubcheck_jar = Some(PathBuf::from(
                    args.next()
                        .ok_or(CliError::MissingValue("--epubcheck-jar"))?,
                ));
            }
            "--progress" => {
                let value = args.next().ok_or(CliError::MissingValue("--progress"))?;
                parsed.progress = parse_progress(&value)?;
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
            Ok(Command::Validate(parsed))
        }
        _ => Err(CliError::ValidateArgs),
    }
}

/// Parse `model <pull|list|remove> [ID]`, having already consumed the subcommand.
fn parse_model<I: Iterator<Item = String>>(mut args: I) -> Result<Command, CliError> {
    let mut positional = Vec::new();
    let mut registry = None;
    let mut dir = None;
    let mut json = false;
    let mut progress = Progress::None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--registry" => {
                registry = Some(PathBuf::from(
                    args.next().ok_or(CliError::MissingValue("--registry"))?,
                ));
            }
            "--dir" => {
                dir = Some(PathBuf::from(
                    args.next().ok_or(CliError::MissingValue("--dir"))?,
                ));
            }
            "--json" => json = true,
            "--progress" => {
                let value = args.next().ok_or(CliError::MissingValue("--progress"))?;
                progress = parse_progress(&value)?;
            }
            "--help" | "-h" => return Ok(Command::Print(USAGE.to_owned())),
            other if other.starts_with('-') => {
                return Err(CliError::UnknownOption(other.to_owned()))
            }
            other => positional.push(other.to_owned()),
        }
    }
    let mut positional = positional.into_iter();
    let action = match positional.next().as_deref() {
        Some("pull") => ModelAction::Pull,
        Some("list") => ModelAction::List,
        Some("remove") => ModelAction::Remove,
        _ => return Err(CliError::ModelArgs),
    };
    let id = positional.next();
    if positional.next().is_some() || (action == ModelAction::List) != id.is_none() {
        return Err(CliError::ModelArgs);
    }
    Ok(Command::Model(ModelArgs {
        action,
        id,
        registry,
        dir,
        json,
        progress,
    }))
}

/// Parse `provider detect | check <URL> | probe <URL>`, having already consumed the subcommand.
fn parse_provider_command<I: Iterator<Item = String>>(mut args: I) -> Result<Command, CliError> {
    let mut positional = Vec::new();
    let mut ai = AiArgs::default();
    let mut json = false;
    let mut progress = Progress::None;
    // Whether a flag only `probe` reads was given.
    let mut probe_only = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--progress" => {
                let value = args.next().ok_or(CliError::MissingValue("--progress"))?;
                progress = parse_progress(&value)?;
            }
            "--llm-provider" => {
                let value = args
                    .next()
                    .ok_or(CliError::MissingValue("--llm-provider"))?;
                ai.provider = Some(parse_provider(&value)?);
                probe_only = true;
            }
            "--llm-model" => {
                ai.model = Some(args.next().ok_or(CliError::MissingValue("--llm-model"))?);
                probe_only = true;
            }
            "--llm-allow-host" => {
                ai.allow_host = Some(
                    args.next()
                        .ok_or(CliError::MissingValue("--llm-allow-host"))?,
                );
                probe_only = true;
            }
            "--llm-api-key-file" => {
                ai.api_key_file = Some(PathBuf::from(
                    args.next()
                        .ok_or(CliError::MissingValue("--llm-api-key-file"))?,
                ));
                probe_only = true;
            }
            "--help" | "-h" => return Ok(Command::Print(USAGE.to_owned())),
            other if other.starts_with('-') => {
                return Err(CliError::UnknownOption(other.to_owned()))
            }
            other => positional.push(other.to_owned()),
        }
    }
    let mut positional = positional.into_iter();
    let action = match positional.next().as_deref() {
        Some("detect") => ProviderAction::Detect,
        Some("check") => ProviderAction::Check,
        Some("probe") => ProviderAction::Probe,
        _ => return Err(CliError::ProviderArgs),
    };
    ai.endpoint = positional.next();
    let wants_url = action != ProviderAction::Detect;
    if positional.next().is_some()
        || ai.endpoint.is_some() != wants_url
        || (probe_only && action != ProviderAction::Probe)
    {
        return Err(CliError::ProviderArgs);
    }
    Ok(Command::Provider(ProviderArgs {
        action,
        ai,
        json,
        progress,
    }))
}

/// A document preset by name (D13.11).
fn parse_preset(value: &str) -> Result<oc_model::document::PresetName, CliError> {
    use oc_model::document::PresetName;
    match value {
        "auto" => Ok(PresetName::Auto),
        "novel" => Ok(PresetName::Novel),
        "academic" => Ok(PresetName::Academic),
        "textbook" => Ok(PresetName::Textbook),
        "poetry" => Ok(PresetName::Poetry),
        "scanned" => Ok(PresetName::Scanned),
        _ => Err(CliError::BadValue {
            what: "--preset value",
            value: value.to_owned(),
        }),
    }
}

/// A provider by the name a user knows it by (UI_UX §2.4): the built-in server is `builtin`.
fn parse_provider(value: &str) -> Result<oc_ai::provider::ProviderKind, CliError> {
    use oc_ai::provider::ProviderKind;
    match value {
        "builtin" => Ok(ProviderKind::LocalSidecar),
        "ollama" => Ok(ProviderKind::Ollama),
        "openai-compatible" => Ok(ProviderKind::OpenAiCompatible),
        _ => Err(CliError::BadValue {
            what: "--llm-provider value",
            value: value.to_owned(),
        }),
    }
}

fn parse_progress(value: &str) -> Result<Progress, CliError> {
    match value {
        "none" => Ok(Progress::None),
        "json" => Ok(Progress::Json),
        _ => Err(CliError::BadValue {
            what: "--progress value",
            value: value.to_owned(),
        }),
    }
}

/// Parse `dump-stage <STAGE> <INPUT>`, having already consumed the subcommand.
///
/// The stage is positional and first because it is not optional: "dump a stage" without
/// saying which is not a request.
fn parse_dump_stage<I: Iterator<Item = String>>(mut args: I) -> Result<Command, CliError> {
    let mut positional = Vec::new();
    let mut parsed = DumpStageArgs {
        stage: String::new(),
        input: PathBuf::new(),
        password: std::env::var("OC_PDF_PASSWORD").ok(),
        progress: Progress::None,
        limits: oc_core::limits::Limits::default(),
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
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
            "--max-pages" => {
                let value = args.next().ok_or(CliError::MissingValue("--max-pages"))?;
                parsed.limits.max_pages = value.parse().map_err(|_| CliError::BadValue {
                    what: "--max-pages value",
                    value,
                })?;
            }
            "--help" | "-h" => return Ok(Command::Print(USAGE.to_owned())),
            other if other.starts_with('-') => {
                return Err(CliError::UnknownOption(other.to_owned()))
            }
            other => positional.push(other.to_owned()),
        }
    }

    match positional.len() {
        2 => {
            parsed.input = PathBuf::from(&positional[1]);
            parsed.stage = positional.remove(0);
            Ok(Command::DumpStage(parsed))
        }
        _ => Err(CliError::DumpStageArgs),
    }
}

/// Parse `diff-stage <STAGE> <INPUT>`, having already consumed the subcommand.
///
/// The same grammar as `dump-stage`, parsed by the same function: two commands that take the
/// same arguments and disagree about how to spell them would be a defect waiting to happen.
fn parse_diff_stage<I: Iterator<Item = String>>(args: I) -> Result<Command, CliError> {
    let parsed = parse_dump_stage(args).map_err(|error| match error {
        CliError::DumpStageArgs => CliError::DiffStageArgs,
        other => other,
    })?;
    Ok(match parsed {
        Command::DumpStage(dump) => Command::DiffStage(DiffStageArgs {
            stage: dump.stage,
            input: dump.input,
            password: dump.password,
            progress: dump.progress,
            limits: dump.limits,
        }),
        // `--help` anywhere in the arguments, which `parse_dump_stage` answers directly.
        other => other,
    })
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
