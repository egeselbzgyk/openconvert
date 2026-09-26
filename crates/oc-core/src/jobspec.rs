//! The job spec: the one argument the desktop app gives the engine (D13.2, RT B15).
//!
//! The app never passes `--input`, `--output` or `--model-path`. It writes one JSON file into a
//! directory it controls and passes that file's path, and everything the job needs is inside it.
//! That is what makes the shell's argument allow-list mean something: the only shape it has to
//! permit is "one path", and the path is only as trustworthy as the validator that reads what is
//! behind it. This module is that validator, and **both sides run it** — the app before it spawns
//! anything (Phase 12 test 12.2) and the engine before it touches the PDF (§2.2, exit code 2).
//!
//! **Validated against the committed schema, not against a transcription of it.** The schema file
//! `schemas/job-spec.v2.json` is compiled in and walked here, so a pattern, a minimum or an
//! `additionalProperties: false` in the schema is enforced because it is in the schema — there is
//! no second copy of `^[0-9a-f]{64}$` or `268435456` in Rust to drift from it. The walker
//! understands exactly the keywords the schema uses and **fails closed on any other**: a schema
//! edit that introduced, say, `maxLength` would make every spec invalid until the walker learned
//! it, rather than silently not checking it. (A schema edit is a new file anyway — §15's
//! versioning table — so this is the loud failure the versioning rule wants.)
//!
//! After the schema, the document is deserialised into [`JobSpec`] for use, and one rule the
//! schema cannot express is applied: an AI endpoint that is not on this machine needs the user's
//! recorded consent (D10, SECURITY §8).

use std::collections::BTreeMap;
use std::path::PathBuf;

use oc_model::document::PresetName;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The committed schema, compiled in.
pub const SCHEMA_TEXT: &str = include_str!("../../../schemas/job-spec.v2.json");

/// The value of the spec's own `schema` field for this version. A version-1 spec — the same
/// fields without `ai.mode` — is still accepted, and reads as the default mode.
pub const SCHEMA_TAG: &str = "openconvert.job/2";

/// Why a job spec was refused. Always exit code 2 and `fatal{code: "E_JOBSPEC"}` (§2.2).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum JobSpecError {
    /// The text is not JSON at all.
    #[error("the job spec is not JSON: {0}")]
    NotJson(String),
    /// The document breaks a rule of the schema, at a JSON-pointer location.
    #[error("the job spec is invalid at {pointer}: {problem}")]
    Invalid { pointer: String, problem: String },
    /// The schema uses a keyword this validator does not implement. Fails closed.
    #[error("the job-spec schema uses `{0}`, which this validator does not implement")]
    UnsupportedKeyword(String),
    /// A non-loopback AI endpoint without recorded consent (D10).
    #[error("the AI endpoint {host} is not on this computer and the job spec records no consent to send document text there")]
    NonLoopbackWithoutConsent { host: String },
}

/// The job, as the engine uses it.
///
/// Mirrors the schema field for field. `input`, `output` and `limits` accept properties they do
/// not know, exactly as the schema does (it declares `additionalProperties: false` only at the top
/// level and on `ai`); the schema, not this struct, is the contract a writer is held to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobSpec {
    pub schema: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    pub input: InputSpec,
    pub output: OutputSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<PresetName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai: Option<AiSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<LimitsSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold_overrides: Option<BTreeMap<String, f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dump_stages: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputSpec {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// A file holding the password, never the password itself: an argument or a spec field is
    /// readable by anything that can list processes or read the job directory's history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_file: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSpec {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_path: Option<PathBuf>,
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiSpec {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default)]
    pub non_loopback_consent: bool,
    /// How much of the model's time a book may take, and how it is asked (v2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<AiMode>,
}

/// How the model is asked, whichever model it is (job spec v2, `--ai-mode`).
///
/// **Fast** asks for a constrained one-word answer and takes a short time budget. **Quality** lets
/// the model reason in its answer before it commits to one, asks each question twice and keeps
/// only answers that agree, and takes a much longer budget. Neither depends on a model family's
/// own switches, so a BYO endpoint and Ollama are asked the same way as the bundled model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiMode {
    Fast,
    #[default]
    Quality,
}

impl AiMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AiMode::Fast => "fast",
            AiMode::Quality => "quality",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitsSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pages: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_deadline_secs: Option<u64>,
}

impl JobSpec {
    /// A minimal spec: the schema tag, an input and an output, nothing else.
    pub fn new(input: PathBuf, output: PathBuf) -> Self {
        Self {
            schema: SCHEMA_TAG.to_owned(),
            job_id: None,
            input: InputSpec {
                path: input,
                sha256: None,
                password_file: None,
            },
            output: OutputSpec {
                path: output,
                report_path: None,
                overwrite: false,
            },
            preset: None,
            ai: None,
            limits: None,
            overrides_path: None,
            threshold_overrides: None,
            locale: None,
            dump_stages: None,
        }
    }

    /// Serialise for writing to disk. The result always passes [`parse`] when `self` came from
    /// [`parse`] or was built from [`JobSpec::new`] with schema-valid values.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Check a spec built in memory — by the app, before it is written — by the same rules the
    /// engine will apply to the file.
    pub fn validate(&self) -> Result<(), JobSpecError> {
        parse(&self.to_json()).map(|_| ())
    }
}

/// Read, validate and type one job spec.
pub fn parse(text: &str) -> Result<JobSpec, JobSpecError> {
    let document: Value =
        serde_json::from_str(text).map_err(|error| JobSpecError::NotJson(error.to_string()))?;
    let schema: Value = serde_json::from_str(SCHEMA_TEXT)
        .map_err(|error| JobSpecError::UnsupportedKeyword(format!("(schema not JSON: {error})")))?;
    validate_value(&schema, &document, "", true)?;

    let spec: JobSpec =
        serde_json::from_value(document).map_err(|error| JobSpecError::Invalid {
            pointer: String::new(),
            problem: error.to_string(),
        })?;
    check_paths(&spec)?;
    check_consent(&spec)?;
    Ok(spec)
}

/// Every path a spec names is absolute and has no `..` in it (PHASE 14 row 14.14, RT B15).
///
/// The schema says only "string". A relative path would be resolved against whatever directory
/// the engine happens to run in, and a `..` defeats every prefix check a supervisor or a sandbox
/// makes on the path ("inside the job directory" is not a property `/jobs/../home/me` has). So the
/// spec's writer — the app, or whatever wrote a file the engine was pointed at — names files
/// exactly, or the spec is refused before anything is read.
fn check_paths(spec: &JobSpec) -> Result<(), JobSpecError> {
    let ai = spec.ai.as_ref();
    let paths = [
        ("/input/path", Some(&spec.input.path)),
        ("/input/password_file", spec.input.password_file.as_ref()),
        ("/output/path", Some(&spec.output.path)),
        ("/output/report_path", spec.output.report_path.as_ref()),
        ("/overrides_path", spec.overrides_path.as_ref()),
        (
            "/ai/api_key_file",
            ai.and_then(|ai| ai.api_key_file.as_ref()),
        ),
        ("/ai/model_path", ai.and_then(|ai| ai.model_path.as_ref())),
    ];
    for (pointer, path) in paths {
        let Some(path) = path else {
            continue;
        };
        if !path.is_absolute() {
            return Err(invalid(pointer, "is not an absolute path"));
        }
        if path
            .components()
            .any(|component| component == std::path::Component::ParentDir)
        {
            return Err(invalid(pointer, "contains `..`"));
        }
    }
    Ok(())
}

/// D10: a non-loopback endpoint needs `non_loopback_consent: true`.
fn check_consent(spec: &JobSpec) -> Result<(), JobSpecError> {
    let Some(ai) = &spec.ai else {
        return Ok(());
    };
    let Some(endpoint) = &ai.endpoint else {
        return Ok(());
    };
    let host = endpoint_host(endpoint);
    if !is_loopback_host(&host) && !ai.non_loopback_consent {
        return Err(JobSpecError::NonLoopbackWithoutConsent { host });
    }
    Ok(())
}

/// The host of a `scheme://host[:port]/…` URL, lowercased, brackets kept off an IPv6 literal.
fn endpoint_host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        stripped.split(']').next().unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or_default()
    };
    host.to_ascii_lowercase()
}

/// Loopback, by name or by address. Everything else leaves the machine (SECURITY §8).
fn is_loopback_host(host: &str) -> bool {
    if host == "localhost" {
        return true;
    }
    host.parse::<core::net::IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}

/// The schema keywords this validator implements. Anything else fails closed.
const KNOWN_KEYWORDS: [&str; 12] = [
    "$schema",
    "type",
    "required",
    "additionalProperties",
    "properties",
    "const",
    "enum",
    "pattern",
    "minimum",
    "format",
    "default",
    "items",
];

fn invalid(pointer: &str, problem: impl Into<String>) -> JobSpecError {
    JobSpecError::Invalid {
        pointer: if pointer.is_empty() {
            "/".to_owned()
        } else {
            pointer.to_owned()
        },
        problem: problem.into(),
    }
}

/// Validate `value` against `schema` at `pointer`.
fn validate_value(
    schema: &Value,
    value: &Value,
    pointer: &str,
    root: bool,
) -> Result<(), JobSpecError> {
    let Some(rules) = schema.as_object() else {
        return Err(JobSpecError::UnsupportedKeyword(
            "a schema that is not an object".to_owned(),
        ));
    };
    for keyword in rules.keys() {
        if !KNOWN_KEYWORDS.contains(&keyword.as_str()) {
            return Err(JobSpecError::UnsupportedKeyword(keyword.clone()));
        }
        // `$schema` is an annotation at the root and meaningless anywhere else.
        if keyword == "$schema" && !root {
            return Err(JobSpecError::UnsupportedKeyword(keyword.clone()));
        }
    }

    if let Some(kind) = rules.get("type") {
        check_type(kind, value, pointer)?;
    }
    if let Some(expected) = rules.get("const") {
        if value != expected {
            return Err(invalid(pointer, format!("must be {expected}")));
        }
    }
    if let Some(options) = rules.get("enum") {
        let options = options
            .as_array()
            .ok_or_else(|| JobSpecError::UnsupportedKeyword("enum (not an array)".to_owned()))?;
        if !options.contains(value) {
            return Err(invalid(
                pointer,
                format!("{value} is not one of {}", Value::Array(options.clone())),
            ));
        }
    }
    if let Some(pattern) = rules.get("pattern") {
        let pattern = pattern
            .as_str()
            .ok_or_else(|| JobSpecError::UnsupportedKeyword("pattern (not a string)".to_owned()))?;
        if let Some(text) = value.as_str() {
            let regex = regex::Regex::new(pattern)
                .map_err(|_| JobSpecError::UnsupportedKeyword(format!("pattern {pattern}")))?;
            if !regex.is_match(text) {
                return Err(invalid(pointer, format!("does not match {pattern}")));
            }
        }
    }
    if let Some(minimum) = rules.get("minimum") {
        let minimum = minimum
            .as_f64()
            .ok_or_else(|| JobSpecError::UnsupportedKeyword("minimum (not a number)".to_owned()))?;
        if let Some(number) = value.as_f64() {
            if number < minimum {
                return Err(invalid(
                    pointer,
                    format!("{value} is below the minimum {minimum}"),
                ));
            }
        }
    }
    if let Some(format) = rules.get("format") {
        match format.as_str() {
            Some("uri") => {
                if let Some(text) = value.as_str() {
                    if !looks_like_uri(text) {
                        return Err(invalid(pointer, "is not an absolute URI"));
                    }
                }
            }
            other => {
                return Err(JobSpecError::UnsupportedKeyword(format!(
                    "format {}",
                    other.unwrap_or("?")
                )))
            }
        }
    }

    if let Some(object) = value.as_object() {
        let properties = rules.get("properties").and_then(Value::as_object);
        if let Some(required) = rules.get("required") {
            let required = required.as_array().ok_or_else(|| {
                JobSpecError::UnsupportedKeyword("required (not an array)".to_owned())
            })?;
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    return Err(invalid(pointer, format!("`{name}` is required")));
                }
            }
        }
        for (name, child) in object {
            let child_pointer = format!("{pointer}/{name}");
            match properties.and_then(|properties| properties.get(name)) {
                Some(child_schema) => validate_value(child_schema, child, &child_pointer, false)?,
                None => match rules.get("additionalProperties") {
                    Some(Value::Bool(false)) => {
                        return Err(invalid(pointer, format!("`{name}` is not a known field")));
                    }
                    Some(Value::Bool(true)) | None => {}
                    Some(additional) => {
                        validate_value(additional, child, &child_pointer, false)?;
                    }
                },
            }
        }
    }

    if let (Some(items), Some(array)) = (rules.get("items"), value.as_array()) {
        for (index, item) in array.iter().enumerate() {
            validate_value(items, item, &format!("{pointer}/{index}"), false)?;
        }
    }
    Ok(())
}

fn check_type(kind: &Value, value: &Value, pointer: &str) -> Result<(), JobSpecError> {
    let kind = kind
        .as_str()
        .ok_or_else(|| JobSpecError::UnsupportedKeyword("type (not a string)".to_owned()))?;
    let matches = match kind {
        "object" => value.is_object(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "number" => value.is_number(),
        // JSON Schema's integer is a number with no fractional part; serde_json keeps the
        // distinction, and `u64`/`i64` is what "no fractional part" means for a limit.
        "integer" => value.is_u64() || value.is_i64(),
        other => return Err(JobSpecError::UnsupportedKeyword(format!("type {other}"))),
    };
    if matches {
        Ok(())
    } else {
        Err(invalid(pointer, format!("must be of type {kind}")))
    }
}

/// `scheme://rest`, with an RFC 3986 scheme and a non-empty rest.
fn looks_like_uri(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once("://") else {
        return false;
    };
    let mut chars = scheme.chars();
    let starts_alpha = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    starts_alpha
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && !rest.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `unix` as an absolute path on this host. A spec's paths must be absolute (`check_paths`),
    /// and on Windows `/tmp/in.pdf` is only rooted: absolute needs a drive, so it gets one.
    fn absolute(unix: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!("C:{unix}"))
        } else {
            PathBuf::from(unix)
        }
    }

    fn minimal() -> Value {
        serde_json::json!({
            "schema": "openconvert.job/1",
            "input": {"path": absolute("/tmp/in.pdf")},
            "output": {"path": absolute("/tmp/out.epub")}
        })
    }

    /// A version-1 spec is still a valid spec, and reads as the default mode; a version-2 spec
    /// may name the mode (`ai.mode`), and nothing else is a mode.
    #[test]
    fn a_version_one_spec_is_accepted_and_a_mode_is_read() {
        let v1 = parse(&minimal().to_string()).expect("a v1 spec is accepted");
        assert_eq!(v1.ai, None);

        let mut fast = minimal();
        fast["schema"] = serde_json::json!(SCHEMA_TAG);
        fast["ai"] = serde_json::json!({"enabled": true, "mode": "fast"});
        let spec = parse(&fast.to_string()).expect("a v2 spec with a mode is accepted");
        assert_eq!(spec.ai.and_then(|ai| ai.mode), Some(AiMode::Fast));

        let mut slow = fast.clone();
        slow["ai"]["mode"] = serde_json::json!("thorough");
        match refused(&slow) {
            JobSpecError::Invalid { pointer, .. } => assert_eq!(pointer, "/ai/mode"),
            other => panic!("expected Invalid, got {other}"),
        }
        assert_eq!(AiMode::default(), AiMode::Quality);
    }

    fn refused(document: &Value) -> JobSpecError {
        parse(&document.to_string()).expect_err("the spec is refused")
    }

    /// `minimal()` with `value` at the JSON pointer `field`, creating the objects on the way.
    /// Its one user is the Unix-only path test below.
    #[cfg(unix)]
    fn with(field: &str, value: Value) -> Value {
        let mut spec = minimal();
        let parts: Vec<&str> = field.trim_start_matches('/').split('/').collect();
        let (last, parents) = parts.split_last().expect("a pointer");
        let mut node = &mut spec;
        for part in parents {
            node = node
                .as_object_mut()
                .expect("an object")
                .entry(*part)
                .or_insert_with(|| serde_json::json!({}));
        }
        node.as_object_mut()
            .expect("an object")
            .insert((*last).to_owned(), value);
        spec
    }

    /// PHASE 14 row 14.14's property as a unit test, and RT B15's reason for it: the one argument
    /// the app passes is only as safe as the validator behind it, and a validator that accepts a
    /// relative path or a `..` has let the spec's writer choose what the engine reads and writes
    /// relative to wherever it runs.
    #[cfg(unix)]
    #[test]
    fn every_path_in_an_accepted_spec_is_absolute_and_never_climbs() {
        for (field, value) in [
            ("/input/path", serde_json::json!("in.pdf")),
            ("/input/path", serde_json::json!("/tmp/../etc/passwd")),
            ("/input/path", serde_json::json!("")),
            ("/output/path", serde_json::json!("../out.epub")),
            ("/output/report_path", serde_json::json!("report.json")),
            ("/input/password_file", serde_json::json!("/tmp/a/../../pw")),
            ("/overrides_path", serde_json::json!("overrides.json")),
            ("/ai/api_key_file", serde_json::json!("key")),
            ("/ai/model_path", serde_json::json!("/m/../../model.gguf")),
        ] {
            let spec = with(field, value.clone());
            match refused(&spec) {
                JobSpecError::Invalid { pointer, .. } => assert_eq!(pointer, field, "{value}"),
                other => panic!("{field} = {value}: expected Invalid, got {other}"),
            }
        }
        // And the ordinary shape still passes.
        assert!(parse(&minimal().to_string()).is_ok());
    }

    #[test]
    fn a_minimal_spec_parses() {
        let spec = parse(&minimal().to_string()).expect("the minimal spec is valid");
        assert_eq!(spec.schema, SCHEMA_TAG);
        assert_eq!(spec.input.path, absolute("/tmp/in.pdf"));
        assert!(!spec.output.overwrite, "overwrite defaults to false");
    }

    #[test]
    fn a_full_spec_round_trips() {
        let mut spec = JobSpec::new(absolute("/a/in.pdf"), absolute("/b/out.epub"));
        spec.job_id = Some("job_01-A".to_owned());
        spec.preset = Some(PresetName::Academic);
        spec.limits = Some(LimitsSpec {
            max_pages: Some(10),
            max_memory_bytes: Some(1 << 30),
            stage_deadline_secs: Some(5),
        });
        spec.locale = Some("tr".to_owned());
        spec.input.sha256 = Some("a".repeat(64));
        let parsed = parse(&spec.to_json()).expect("what the app writes, the engine reads");
        assert_eq!(parsed, spec);
    }

    #[test]
    fn the_schema_is_what_is_enforced() {
        // Each of these is a rule that lives only in the schema file.
        let mut wrong_tag = minimal();
        wrong_tag["schema"] = "openconvert.job/2".into();
        assert!(matches!(refused(&wrong_tag), JobSpecError::Invalid { .. }));

        let mut bad_id = minimal();
        bad_id["job_id"] = "has space".into();
        assert!(
            matches!(refused(&bad_id), JobSpecError::Invalid { pointer, .. } if pointer == "/job_id")
        );

        let mut bad_hash = minimal();
        bad_hash["input"]["sha256"] = "ABC".into();
        assert!(matches!(refused(&bad_hash), JobSpecError::Invalid { .. }));

        let mut low_memory = minimal();
        low_memory["limits"] = serde_json::json!({"max_memory_bytes": 1024});
        assert!(matches!(refused(&low_memory), JobSpecError::Invalid { .. }));

        let mut zero_pages = minimal();
        zero_pages["limits"] = serde_json::json!({"max_pages": 0});
        assert!(matches!(refused(&zero_pages), JobSpecError::Invalid { .. }));

        let mut fractional = minimal();
        fractional["limits"] = serde_json::json!({"max_pages": 1.5});
        assert!(matches!(refused(&fractional), JobSpecError::Invalid { .. }));

        let mut preset = minimal();
        preset["preset"] = "comic".into();
        assert!(matches!(refused(&preset), JobSpecError::Invalid { .. }));

        let mut extra = minimal();
        extra["argv"] = serde_json::json!(["--model-path", "/x"]);
        assert!(matches!(refused(&extra), JobSpecError::Invalid { .. }));

        let mut extra_ai = minimal();
        extra_ai["ai"] = serde_json::json!({"enabled": false, "temperature": 1.0});
        assert!(matches!(refused(&extra_ai), JobSpecError::Invalid { .. }));

        let mut missing = minimal();
        missing.as_object_mut().map(|o| o.remove("output"));
        assert!(matches!(refused(&missing), JobSpecError::Invalid { .. }));

        let mut uri = minimal();
        uri["ai"] = serde_json::json!({"endpoint": "not a uri"});
        assert!(matches!(refused(&uri), JobSpecError::Invalid { .. }));
    }

    #[test]
    fn nested_objects_admit_what_the_schema_admits() {
        // The schema leaves `input`, `output` and `limits` open; so does the parser.
        let mut open = minimal();
        open["input"]["note"] = "kept".into();
        parse(&open.to_string()).expect("input is not additionalProperties:false in the schema");
    }

    #[test]
    fn a_non_loopback_endpoint_needs_consent() {
        let mut remote = minimal();
        remote["ai"] =
            serde_json::json!({"enabled": true, "endpoint": "https://api.example.com/v1"});
        assert_eq!(
            refused(&remote),
            JobSpecError::NonLoopbackWithoutConsent {
                host: "api.example.com".to_owned()
            }
        );

        remote["ai"]["non_loopback_consent"] = true.into();
        parse(&remote.to_string()).expect("consent recorded");

        for local in [
            "http://localhost:11434",
            "http://127.0.0.1:8080/v1",
            "http://[::1]:9000",
        ] {
            let mut spec = minimal();
            spec["ai"] = serde_json::json!({"enabled": true, "endpoint": local});
            parse(&spec.to_string()).expect("loopback needs no consent");
        }
    }

    #[test]
    fn an_unknown_schema_keyword_fails_closed() {
        let schema = serde_json::json!({"type": "string", "maxLength": 3});
        assert_eq!(
            validate_value(&schema, &"abcdef".into(), "", false),
            Err(JobSpecError::UnsupportedKeyword("maxLength".to_owned()))
        );
    }

    #[test]
    fn not_json_is_named() {
        assert!(matches!(parse("%PDF-1.7"), Err(JobSpecError::NotJson(_))));
    }
}
