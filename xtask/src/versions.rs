//! `cargo xtask bump-rules-check` — the version-bump rules of `docs/VERSIONING.md`, enforced
//! (PHASE 15 detail 8; rows 15.16, 15.17; A15.7).
//!
//! A committed baseline (`docs/releases/baseline.toml`) records, for the last release, each
//! version and a digest of what that version guards. The check recomputes the digests from the
//! tree and fails when a guarded thing changed and its version did not move:
//!
//! | version | its digest covers |
//! |---|---|
//! | `ir_version` | every type, alias and constant in `oc-model`, and the block-id derivation and canonical serialisation (`ids.rs`, `canonical.rs`) in full — an IR field added, removed or renamed, a `Reason` variant added, a derivation changed |
//! | `protocol` | the event constructors in `oc-core/src/events.rs`, every `.emit("<type>", …)` in `oc-core` and `openconvert`, and the control channel (`openconvert/src/control.rs`) |
//! | `prompt_version` | every byte under `crates/oc-ai/prompts/` |
//! | `job_spec_schema` | `schemas/job-spec.v<N>.json`, which may never change: a change is a new file |
//!
//! The digests are of the *code as the compiler sees it*: comments, doc comments, formatting and
//! `#[cfg(test)]` items are left out, so a reworded comment or a new test owes nobody a bump, while
//! a renamed field, a new variant or an edited payload does. This is the only mechanical defence
//! against the most expensive silent mistake in the system: an IR change that leaves every
//! user's caches and `overrides.json` claiming to fit an IR they no longer fit.
//!
//! Before the first release the baseline says `released = "none"`: nothing has shipped that a
//! change could break, so drift is reported and not failed, and the release job records the
//! baseline when it tags. After that, the check is strict.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Where the baseline lives, relative to the workspace root.
pub const BASELINE: &str = "docs/releases/baseline.toml";

/// `released` before any release has been tagged.
pub const UNRELEASED: &str = "none";

/// The versions a release declares (PHASE 15 Architecture).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Versions {
    pub app: String,
    pub ir_version: u32,
    pub protocol: u32,
    pub prompt_version: u32,
    pub job_spec_schema: u32,
}

/// What each version guards, as a digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeLayoutDigest {
    pub ir: String,
    pub protocol: String,
    pub prompt: String,
    /// `schemas/job-spec.v<job_spec_schema>.json`, byte for byte.
    pub job_spec: String,
}

/// The committed record of the last release.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    /// The tag it was recorded at, or [`UNRELEASED`].
    pub released: String,
    pub versions: Versions,
    pub digests: TypeLayoutDigest,
}

/// One broken rule.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BumpViolation {
    #[error("{what} changed since {released} and {version} is still {value}: bump it (docs/VERSIONING.md)")]
    NotBumped {
        what: &'static str,
        version: &'static str,
        value: u32,
        released: String,
    },
    #[error("{version} went backwards: {released} shipped {before}, the tree declares {now}")]
    Backwards {
        version: &'static str,
        before: u32,
        now: u32,
        released: String,
    },
    #[error(
        "schemas/job-spec.v{version}.json was edited after {released} shipped it; a job-spec change \
         is a new file schemas/job-spec.v{next}.json and the old one is never mutated",
        next = version + 1
    )]
    JobSpecMutated { version: u32, released: String },
}

/// The rules, over the last release's record and the tree's digests and versions.
/// `old_job_spec` is the digest the tree has *now* for the job-spec schema file the baseline
/// released (`None` when that file is gone).
pub fn bump_rules_check(
    prev: &Baseline,
    now: &TypeLayoutDigest,
    declared: &Versions,
    old_job_spec: Option<&str>,
) -> Result<(), Vec<BumpViolation>> {
    let mut violations = Vec::new();
    let released = prev.released.clone();
    let rules: [(&'static str, &'static str, &String, &String, u32, u32); 3] = [
        (
            "the IR's type layout",
            "ir_version",
            &prev.digests.ir,
            &now.ir,
            prev.versions.ir_version,
            declared.ir_version,
        ),
        (
            "the event protocol",
            "protocol",
            &prev.digests.protocol,
            &now.protocol,
            prev.versions.protocol,
            declared.protocol,
        ),
        (
            "a prompt or grammar",
            "prompt_version",
            &prev.digests.prompt,
            &now.prompt,
            prev.versions.prompt_version,
            declared.prompt_version,
        ),
    ];
    for (what, version, before, after, old, new) in rules {
        if new < old {
            violations.push(BumpViolation::Backwards {
                version,
                before: old,
                now: new,
                released: released.clone(),
            });
        } else if before != after && new == old {
            violations.push(BumpViolation::NotBumped {
                what,
                version,
                value: new,
                released: released.clone(),
            });
        }
    }
    let (old, new) = (prev.versions.job_spec_schema, declared.job_spec_schema);
    if new < old {
        violations.push(BumpViolation::Backwards {
            version: "job_spec_schema",
            before: old,
            now: new,
            released: released.clone(),
        });
    } else if old_job_spec != Some(prev.digests.job_spec.as_str()) {
        violations.push(BumpViolation::JobSpecMutated {
            version: old,
            released,
        });
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

// ---------------------------------------------------------------------------------------------
// Digests
// ---------------------------------------------------------------------------------------------

/// A token stream with every doc attribute (`#[doc = …]`, `#![doc = …]`, which is what `///` and
/// `//!` become) removed, recursively. Comments are already gone once the source is parsed.
fn strip_docs(stream: TokenStream) -> TokenStream {
    let tokens: Vec<TokenTree> = stream.into_iter().collect();
    let mut out = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        if let TokenTree::Punct(hash) = &tokens[i] {
            if hash.as_char() == '#' {
                let bang =
                    matches!(tokens.get(i + 1), Some(TokenTree::Punct(p)) if p.as_char() == '!');
                let at = if bang { i + 2 } else { i + 1 };
                if let Some(TokenTree::Group(group)) = tokens.get(at) {
                    let is_doc = group.delimiter() == Delimiter::Bracket
                        && matches!(group.stream().into_iter().next(),
                                    Some(TokenTree::Ident(ident)) if ident == "doc");
                    if is_doc {
                        i = at + 1;
                        continue;
                    }
                }
            }
        }
        out.push(match &tokens[i] {
            TokenTree::Group(group) => {
                let mut inner =
                    proc_macro2::Group::new(group.delimiter(), strip_docs(group.stream()));
                inner.set_span(group.span());
                TokenTree::Group(inner)
            }
            other => other.clone(),
        });
        i += 1;
    }
    out.into_iter().collect()
}

/// Whether an item is test-only: `#[cfg(test)]` or `#[test]`.
fn is_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg")
                && attr
                    .meta
                    .require_list()
                    .is_ok_and(|list| list.tokens.to_string() == "test"))
    })
}

fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(i) => &i.attrs,
        syn::Item::Enum(i) => &i.attrs,
        syn::Item::Fn(i) => &i.attrs,
        syn::Item::Impl(i) => &i.attrs,
        syn::Item::Mod(i) => &i.attrs,
        syn::Item::Static(i) => &i.attrs,
        syn::Item::Struct(i) => &i.attrs,
        syn::Item::Trait(i) => &i.attrs,
        syn::Item::Type(i) => &i.attrs,
        syn::Item::Union(i) => &i.attrs,
        syn::Item::Use(i) => &i.attrs,
        _ => &[],
    }
}

/// The normalised text of the non-test items of `source` that `keep` selects; inline modules
/// are descended into.
fn items_text(source: &str, keep: &dyn Fn(&syn::Item) -> bool) -> Result<String> {
    let file = syn::parse_file(source).context("not Rust syn can parse")?;
    let mut out = String::new();
    collect_items(&file.items, keep, &mut out);
    Ok(out)
}

fn collect_items(items: &[syn::Item], keep: &dyn Fn(&syn::Item) -> bool, out: &mut String) {
    for item in items {
        if is_test(item_attrs(item)) {
            continue;
        }
        if let syn::Item::Mod(module) = item {
            if let Some((_, inner)) = &module.content {
                collect_items(inner, keep, out);
            }
            continue;
        }
        if keep(item) {
            out.push_str(&strip_docs(item.to_token_stream()).to_string());
            out.push('\n');
        }
    }
}

/// Every `.rs` file under `dir`, as (path relative to `root`, text), in a fixed order.
fn rust_files(root: &Path, dir: &str) -> Result<Vec<(String, String)>> {
    let mut files = Vec::new();
    let mut stack = vec![root.join(dir)];
    while let Some(dir) = stack.pop() {
        for entry in
            std::fs::read_dir(&dir).with_context(|| format!("cannot list {}", dir.display()))?
        {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("cannot read {}", path.display()))?;
                files.push((relative, text));
            }
        }
    }
    files.sort();
    Ok(files)
}

fn is_named_const(item: &syn::Item, name: &str) -> bool {
    matches!(item, syn::Item::Const(c) if c.ident == name)
}

fn hex(hasher: Sha256) -> String {
    format!("{:x}", hasher.finalize())
}

/// `oc-model`'s layout: its types, aliases and constants everywhere (less `IR_VERSION`, which is
/// compared on its own), and all of `ids.rs` and `canonical.rs` — how a block id is derived and how
/// the IR is written are part of what `ir_version` promises.
pub fn ir_digest(root: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    for (path, text) in rust_files(root, "crates/oc-model/src")? {
        let whole = path.ends_with("/ids.rs") || path.ends_with("/canonical.rs");
        let keep = |item: &syn::Item| {
            if is_named_const(item, "IR_VERSION") {
                return false;
            }
            whole
                || matches!(
                    item,
                    syn::Item::Struct(_)
                        | syn::Item::Enum(_)
                        | syn::Item::Type(_)
                        | syn::Item::Union(_)
                        | syn::Item::Const(_)
                        | syn::Item::Static(_)
                )
        };
        // A file with nothing but tests in it contributes nothing, its name included.
        let items = items_text(&text, &keep).with_context(|| path.clone())?;
        if !items.is_empty() {
            hasher.update(path.as_bytes());
            hasher.update(items.as_bytes());
        }
    }
    Ok(hex(hasher))
}

/// Every `.emit("<literal>", …)` call in `stream`, as text.
fn emit_calls(stream: TokenStream, out: &mut Vec<String>) {
    let tokens: Vec<TokenTree> = stream.into_iter().collect();
    for (i, token) in tokens.iter().enumerate() {
        match token {
            TokenTree::Ident(ident) if ident == "emit" => {
                let after_dot =
                    i > 0 && matches!(&tokens[i - 1], TokenTree::Punct(p) if p.as_char() == '.');
                if let (true, Some(TokenTree::Group(args))) = (after_dot, tokens.get(i + 1)) {
                    let first = args.stream().into_iter().next();
                    let literal = matches!(&first, Some(TokenTree::Literal(l)) if l.to_string().starts_with('"'));
                    if args.delimiter() == Delimiter::Parenthesis && literal {
                        out.push(format!("emit{}", strip_docs(args.stream())));
                    }
                }
            }
            TokenTree::Group(group) => emit_calls(group.stream(), out),
            _ => {}
        }
    }
}

/// The event protocol: `events.rs` (less `PROTOCOL_VERSION`), the control channel, and every
/// event emitted by name anywhere in `oc-core` or `openconvert`.
pub fn protocol_digest(root: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    let whole = |item: &syn::Item| !is_named_const(item, "PROTOCOL_VERSION");
    for path in [
        "crates/oc-core/src/events.rs",
        "crates/openconvert/src/control.rs",
    ] {
        let text = std::fs::read_to_string(root.join(path))
            .with_context(|| format!("cannot read {path}"))?;
        hasher.update(path.as_bytes());
        hasher.update(items_text(&text, &whole)?.as_bytes());
    }
    for dir in ["crates/oc-core/src", "crates/openconvert/src"] {
        for (path, text) in rust_files(root, dir)? {
            if path.ends_with("/events.rs") {
                continue;
            }
            let file = syn::parse_file(&text).with_context(|| path.clone())?;
            let mut calls = Vec::new();
            for item in &file.items {
                if !is_test(item_attrs(item)) {
                    emit_calls(item.to_token_stream(), &mut calls);
                }
            }
            if !calls.is_empty() {
                hasher.update(path.as_bytes());
                for call in calls {
                    hasher.update(call.as_bytes());
                    hasher.update(b"\n");
                }
            }
        }
    }
    Ok(hex(hasher))
}

/// Every byte under the prompts directory, with every path.
pub fn prompt_digest(root: &Path) -> Result<String> {
    let base = root.join("crates/oc-ai/prompts");
    let mut files = Vec::new();
    let mut stack = vec![base.clone()];
    while let Some(dir) = stack.pop() {
        for entry in
            std::fs::read_dir(&dir).with_context(|| format!("cannot list {}", dir.display()))?
        {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut hasher = Sha256::new();
    for path in files {
        let relative = path
            .strip_prefix(&base)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        hasher.update(relative.as_bytes());
        hasher.update(std::fs::read(&path)?);
    }
    Ok(hex(hasher))
}

/// `schemas/job-spec.v<version>.json`'s SHA-256, or `None` when there is no such file.
pub fn job_spec_digest(root: &Path, version: u32) -> Option<String> {
    let bytes = std::fs::read(root.join(format!("schemas/job-spec.v{version}.json"))).ok()?;
    Some(format!("{:x}", Sha256::digest(&bytes)))
}

/// A `const NAME: u32 = N;` from a source file.
fn const_u32(root: &Path, path: &str, name: &str) -> Result<u32> {
    let text =
        std::fs::read_to_string(root.join(path)).with_context(|| format!("cannot read {path}"))?;
    let file = syn::parse_file(&text).with_context(|| path.to_owned())?;
    for item in &file.items {
        if let syn::Item::Const(c) = item {
            if c.ident == name {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Int(value),
                    ..
                }) = &*c.expr
                {
                    return value
                        .base10_parse()
                        .with_context(|| format!("{name} in {path}"));
                }
            }
        }
    }
    bail!("no `const {name}: u32 = <literal>` in {path}")
}

/// The versions the tree declares, read from the source rather than from what this binary was
/// compiled against, so the check is about the tree it is pointed at.
pub fn declared_versions(root: &Path) -> Result<Versions> {
    let jobspec_path = "crates/oc-core/src/jobspec.rs";
    let jobspec = std::fs::read_to_string(root.join(jobspec_path))
        .with_context(|| format!("cannot read {jobspec_path}"))?;
    let marker = "schemas/job-spec.v";
    let job_spec_schema = jobspec
        .find(marker)
        .map(|at| &jobspec[at + marker.len()..])
        .and_then(|rest| rest.split('.').next())
        .and_then(|n| n.parse().ok())
        .context("oc-core does not compile in a schemas/job-spec.v<N>.json")?;
    let manifest: toml::Table =
        toml::from_str(&std::fs::read_to_string(root.join("Cargo.toml")).context("Cargo.toml")?)?;
    Ok(Versions {
        app: manifest["workspace"]["package"]["version"]
            .as_str()
            .context("no workspace version")?
            .to_owned(),
        ir_version: const_u32(root, "crates/oc-model/src/lib.rs", "IR_VERSION")?,
        protocol: const_u32(root, "crates/oc-core/src/events.rs", "PROTOCOL_VERSION")?,
        prompt_version: const_u32(root, "crates/oc-ai/src/prompt/mod.rs", "PROMPT_VERSION")?,
        job_spec_schema,
    })
}

/// The tree's digests, for the job-spec schema the tree declares.
pub fn digests(root: &Path, declared: &Versions) -> Result<TypeLayoutDigest> {
    Ok(TypeLayoutDigest {
        ir: ir_digest(root)?,
        protocol: protocol_digest(root)?,
        prompt: prompt_digest(root)?,
        job_spec: job_spec_digest(root, declared.job_spec_schema)
            .with_context(|| format!("no schemas/job-spec.v{}.json", declared.job_spec_schema))?,
    })
}

fn read_baseline(root: &Path) -> Result<Baseline> {
    let path = root.join(BASELINE);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("{} is not a baseline", path.display()))
}

/// The baseline file's text for `baseline`.
pub fn baseline_text(baseline: &Baseline) -> Result<String> {
    Ok(format!(
        "# The versions and guarded digests of the last release (docs/VERSIONING.md, PHASE 15 detail 8).\n\
         # Written by `cargo run -p xtask -- bump-rules-check --record <tag>` once a release is tagged,\n\
         # and by nothing else; read by `bump-rules-check` in CI and in the release job.\n\
         # `released = \"{UNRELEASED}\"` until the first release: nothing has shipped, so drift is reported, not failed.\n{}",
        toml::to_string(baseline)?
    ))
}

pub fn run(root: &Path, args: &[String]) -> Result<()> {
    let declared = declared_versions(root)?;
    let now = digests(root, &declared)?;
    if let Some(at) = args.iter().position(|a| a == "--record") {
        let tag = args.get(at + 1).context("--record <tag>")?;
        let baseline = Baseline {
            released: tag.clone(),
            versions: declared,
            digests: now,
        };
        let path = root.join(BASELINE);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, baseline_text(&baseline)?)
            .with_context(|| format!("cannot write {}", path.display()))?;
        println!("recorded the {tag} baseline in {BASELINE}");
        return Ok(());
    }

    let prev = read_baseline(root)?;
    if let Some(at) = args.iter().position(|a| a == "--tag") {
        let tag = args.get(at + 1).context("--tag <vX.Y.Z>")?;
        if tag.trim_start_matches('v') != declared.app {
            bail!(
                "the tag {tag} is not the version the tree declares ({})",
                declared.app
            );
        }
    }
    let old_job_spec = job_spec_digest(root, prev.versions.job_spec_schema);
    let result = bump_rules_check(&prev, &now, &declared, old_job_spec.as_deref());
    match result {
        Ok(()) => {
            println!(
                "bump-rules-check: clean against {} ({declared:?})",
                prev.released
            );
            Ok(())
        }
        Err(violations) if prev.released == UNRELEASED => {
            for violation in &violations {
                println!("bump-rules-check: note (nothing released yet): {violation}");
            }
            println!("bump-rules-check: no release has shipped, so no bump is owed; the first release records the baseline");
            Ok(())
        }
        Err(violations) => {
            for violation in &violations {
                eprintln!("bump-rules-check: {violation}");
            }
            bail!(
                "{} version-bump rule(s) broken (docs/VERSIONING.md)",
                violations.len()
            )
        }
    }
}

/// Where a test copy of the guarded sources goes.
pub fn guarded_paths() -> [&'static str; 8] {
    [
        "Cargo.toml",
        "crates/oc-model/src",
        "crates/oc-core/src",
        "crates/openconvert/src",
        "crates/oc-ai/prompts",
        "crates/oc-ai/src/prompt/mod.rs",
        "schemas",
        BASELINE,
    ]
}

/// Copy `relative` (a file or a directory) from `from` to `to`.
pub fn copy_tree(from: &Path, to: &Path, relative: &str) -> Result<()> {
    let source = from.join(relative);
    let target = to.join(relative);
    if source.is_dir() {
        for entry in std::fs::read_dir(&source)? {
            let name = entry?.file_name();
            let child: PathBuf = Path::new(relative).join(&name);
            copy_tree(from, to, &child.to_string_lossy())?;
        }
        Ok(())
    } else {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&source, &target)
            .with_context(|| format!("cannot copy {}", source.display()))?;
        Ok(())
    }
}
