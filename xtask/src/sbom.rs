//! `cargo xtask sbom` — one CycloneDX 1.6 document per release (PHASE 15 detail 6, SECURITY §9,
//! LICENSE_AND_DEPENDENCIES §5; rows 15.11, 15.12).
//!
//! Three sources, merged:
//!
//! 1. `cargo cyclonedx` over the two crates that ship — the engine (`openconvert`) and the shell
//!    (`openconvert-desktop`) — with their default features, for every target the release builds;
//! 2. `npm sbom --sbom-format cyclonedx` over the UI (`apps/desktop/ui`), whose bundle is built into
//!    the shell;
//! 3. the vendored natives as first-class components, because they are precisely what a
//!    downstream tracker cannot discover from `Cargo.lock` or `package-lock.json`: PDFium (build
//!    number, SHA-256, source URL) and `llama-server` (the `b<N>` tag, SHA-256, source URL) for every
//!    shipped target, and every pack `packs.toml` has actually pinned — none in v1, whose OCR uses
//!    the system's Tesseract (D4).
//!
//! The document is reproducible: no timestamp, a serial number derived from its own content, the
//! build machine's paths removed from every reference, and components in a fixed order. It is
//! validated against the vendored CycloneDX 1.6 schema (`xtask/schemas/cyclonedx/`) before it is
//! written, offline.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::{fetch_llama_server, vendor_pdfium};

/// The CycloneDX version the release SBOM is.
pub const SPEC_VERSION: &str = "1.6";

/// Where the vendored schema lives, relative to the workspace root.
pub const SCHEMA_DIR: &str = "xtask/schemas/cyclonedx";

/// The crates whose builds ship, and the manifests `cargo cyclonedx` is pointed at.
const SHIPPED_CRATES: [(&str, &str); 2] = [
    ("openconvert", "crates/openconvert"),
    ("openconvert-desktop", "apps/desktop/src-tauri"),
];

/// The UI, whose built bundle the shell embeds.
const UI_DIR: &str = "apps/desktop/ui";

/// The target triples a release ships for (D12): Linux x64, macOS on both architectures, Windows
/// x64. A native pinned for another triple is not in any installer, so not in the SBOM.
pub const SHIPPED_TRIPLES: [&str; 4] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-msvc",
];

/// A binary the release carries that no package manager knows about (PHASE 15 Architecture).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeComponent {
    pub name: String,
    pub version: String,
    pub sha256: String,
    pub license: String,
    pub source_url: String,
    /// The target triple it is shipped for, or `any` for a pack.
    pub target: String,
}

/// Every vendored native, from the locks and `packs.toml`.
pub fn natives(workspace_root: &Path) -> Result<Vec<NativeComponent>> {
    let mut out = Vec::new();

    // PDFium: BSD-3-Clause (D3, LICENSE_AND_DEPENDENCIES §2).
    let pdfium = vendor_pdfium::lock()?;
    for asset in &pdfium.assets {
        if !SHIPPED_TRIPLES.contains(&asset.triple.as_str()) {
            continue;
        }
        out.push(NativeComponent {
            name: "pdfium".to_owned(),
            version: format!("{} ({})", pdfium.build, pdfium.version),
            sha256: asset.sha256.clone(),
            license: "BSD-3-Clause".to_owned(),
            source_url: format!(
                "https://github.com/{}/releases/download/{}/{}",
                pdfium.repo,
                pdfium.tag.replace('/', "%2F"),
                asset.asset
            ),
            target: asset.triple.clone(),
        });
    }

    // llama-server: MIT (D8, LICENSE_AND_DEPENDENCIES §2.2).
    let llama = fetch_llama_server::lock()?;
    for triple in SHIPPED_TRIPLES {
        let asset = fetch_llama_server::pinned(&llama, triple)?;
        out.push(NativeComponent {
            name: "llama-server".to_owned(),
            version: llama.tag.clone(),
            sha256: asset.sha256.clone(),
            license: "MIT".to_owned(),
            source_url: format!(
                "https://github.com/{}/releases/download/{}/{}",
                llama.repo, llama.tag, asset.name
            ),
            target: triple.to_owned(),
        });
    }

    // A pack is listed once it is really pinned; a `TODO_` placeholder means it does not ship.
    let packs_path = workspace_root.join("packs.toml");
    let packs: toml::Table = toml::from_str(
        &std::fs::read_to_string(&packs_path)
            .with_context(|| format!("cannot read {}", packs_path.display()))?,
    )
    .context("packs.toml is not valid TOML")?;
    for pack in packs
        .get("pack")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
    {
        let field = |key: &str| pack.get(key).and_then(|v| v.as_str()).unwrap_or_default();
        let pinned = [field("sha256"), field("license"), field("revision")]
            .iter()
            .all(|v| !v.is_empty() && !v.starts_with("TODO_"));
        if !pinned {
            continue;
        }
        out.push(NativeComponent {
            name: field("id").to_owned(),
            version: field("revision").to_owned(),
            sha256: field("sha256").to_owned(),
            license: field("license").to_owned(),
            source_url: format!("https://huggingface.co/{}", field("repo")),
            target: "any".to_owned(),
        });
    }
    Ok(out)
}

fn native_component(native: &NativeComponent) -> Value {
    json!({
        "type": if native.name == "pdfium" { "library" } else { "application" },
        "bom-ref": format!("native:{}@{}", native.name, native.target),
        "name": native.name,
        "version": native.version,
        "hashes": [{ "alg": "SHA-256", "content": native.sha256 }],
        "licenses": [{ "license": { "id": native.license } }],
        "externalReferences": [{ "type": "distribution", "url": native.source_url }],
        "properties": [
            { "name": "openconvert:target", "value": native.target },
            { "name": "openconvert:vendored", "value": "true" },
        ],
    })
}

/// Replace the build machine's absolute workspace path in every string, so the document names
/// `path+file://./crates/oc-core#0.1.0` wherever it was generated.
fn strip_root(value: &mut Value, root: &str) {
    match value {
        Value::String(text) => {
            if text.contains(root) {
                *text = text.replace(root, ".");
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| strip_root(v, root)),
        Value::Object(map) => map.values_mut().for_each(|v| strip_root(v, root)),
        _ => {}
    }
}

/// The components of one input document, its own top-level component included, keyed by
/// `bom-ref`; nested per-target subcomponents are dropped (they name no dependency).
fn collect(
    doc: &Value,
    components: &mut BTreeMap<String, Value>,
    dependencies: &mut BTreeMap<String, Vec<String>>,
) -> Option<String> {
    let top = doc["metadata"]["component"].clone();
    let top_ref = top["bom-ref"].as_str().map(str::to_owned);
    let mut all: Vec<Value> = doc["components"].as_array().cloned().unwrap_or_default();
    if top.is_object() {
        all.push(top);
    }
    for mut component in all {
        if let Some(map) = component.as_object_mut() {
            map.remove("components");
        }
        if let Some(reference) = component["bom-ref"].as_str().map(str::to_owned) {
            components.entry(reference).or_insert(component);
        }
    }
    for dependency in doc["dependencies"].as_array().into_iter().flatten() {
        let Some(reference) = dependency["ref"].as_str() else {
            continue;
        };
        let entry = dependencies.entry(reference.to_owned()).or_default();
        for on in dependency["dependsOn"].as_array().into_iter().flatten() {
            if let Some(on) = on.as_str() {
                entry.push(on.to_owned());
            }
        }
    }
    top_ref
}

/// Merge the crates' and the UI's documents and the natives into one CycloneDX 1.6 document for
/// release `app_version`. `root` is the workspace path the inputs were generated under.
pub fn merge(
    cargo: &[Value],
    npm: &Value,
    natives: &[NativeComponent],
    app_version: &str,
    root: &str,
) -> Value {
    let mut components = BTreeMap::new();
    let mut dependencies: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut parts = Vec::new();
    for doc in cargo.iter().chain(std::iter::once(npm)) {
        let mut doc = doc.clone();
        strip_root(&mut doc, root);
        if let Some(top) = collect(&doc, &mut components, &mut dependencies) {
            parts.push(top);
        }
    }
    for native in natives {
        let component = native_component(native);
        let reference = component["bom-ref"].as_str().unwrap_or_default().to_owned();
        parts.push(reference.clone());
        components.insert(reference, component);
    }

    let app_ref = format!("openconvert-app@{app_version}");
    dependencies.insert(app_ref.clone(), parts);
    // Only references to components the document holds, each once, in order: a dependency on
    // something another tool listed but no input described would fail the schema's intent.
    let known: std::collections::BTreeSet<String> = components
        .keys()
        .cloned()
        .chain(std::iter::once(app_ref.clone()))
        .collect();
    let dependencies: Vec<Value> = dependencies
        .into_iter()
        .filter(|(reference, _)| known.contains(reference))
        .map(|(reference, mut on)| {
            on.retain(|o| known.contains(o));
            on.sort();
            on.dedup();
            json!({ "ref": reference, "dependsOn": on })
        })
        .collect();

    let mut doc = json!({
        "$schema": "http://cyclonedx.org/schema/bom-1.6.schema.json",
        "bomFormat": "CycloneDX",
        "specVersion": SPEC_VERSION,
        "version": 1,
        "metadata": {
            "tools": { "components": [
                { "type": "application", "name": "cargo-cyclonedx" },
                { "type": "application", "name": "npm sbom" },
                { "type": "application", "name": "xtask sbom" },
            ] },
            "component": {
                "type": "application",
                "bom-ref": app_ref,
                "name": "OpenConvert",
                "version": app_version,
                "licenses": [{ "license": { "id": "Apache-2.0" } }],
                "externalReferences": [
                    { "type": "vcs", "url": "https://github.com/openconvert/openconvert" }
                ],
            },
        },
        "components": components.into_values().collect::<Vec<_>>(),
        "dependencies": dependencies,
    });
    // A serial number derived from the content, so the same release always has the same one.
    let digest = serde_json::to_string(&doc).unwrap_or_default();
    let serial = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, digest.as_bytes());
    doc["serialNumber"] = json!(format!("urn:uuid:{serial}"));
    doc
}

/// Every schema violation in `bom`, validated offline against the vendored CycloneDX 1.6 schema.
pub fn validate(bom: &Value, schema_dir: &Path) -> Result<Vec<String>> {
    let read = |name: &str| -> Result<Value> {
        let path = schema_dir.join(name);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("{} is not JSON", path.display()))
    };
    let schema = read("bom-1.6.schema.json")?;
    let mut known = BTreeMap::new();
    for name in ["spdx.schema.json", "jsf-0.82.schema.json"] {
        known.insert(format!("http://cyclonedx.org/schema/{name}"), read(name)?);
    }
    let validator = jsonschema::options()
        .with_retriever(Vendored(known))
        .should_validate_formats(true)
        .build(&schema)
        .map_err(|e| anyhow::anyhow!("the CycloneDX schema does not compile: {e}"))?;
    Ok(validator
        .iter_errors(bom)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect())
}

/// The two schemas the BOM schema refers to, served from the vendored copies; anything else is
/// refused rather than fetched.
struct Vendored(BTreeMap<String, Value>);

impl jsonschema::Retrieve for Vendored {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> std::result::Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        self.0
            .get(uri.as_str())
            .cloned()
            .ok_or_else(|| format!("{uri} is not vendored in {SCHEMA_DIR}").into())
    }
}

fn read_json(path: &Path) -> Result<Value> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("{} is not JSON", path.display()))
}

/// Run `cargo cyclonedx` for one shipped crate and return its document, removing every file the
/// tool wrote into the source tree.
fn cargo_bom(workspace_root: &Path, name: &str, dir: &str) -> Result<Value> {
    let status = Command::new("cargo")
        .args(["cyclonedx", "--format", "json", "--spec-version", "1.5"])
        .args(["--target", "all", "-q", "--manifest-path"])
        .arg(workspace_root.join(dir).join("Cargo.toml"))
        .current_dir(workspace_root)
        .status()
        .context("cannot run `cargo cyclonedx` (cargo install cargo-cyclonedx --locked)")?;
    let wanted = workspace_root.join(dir).join(format!("{name}.cdx.json"));
    let doc = if status.success() {
        read_json(&wanted)
    } else {
        Err(anyhow::anyhow!("`cargo cyclonedx` failed for {name}"))
    };
    // It writes one file per workspace member; none of them belongs in the tree.
    for member in crate_dirs(workspace_root)? {
        for entry in std::fs::read_dir(&member).into_iter().flatten().flatten() {
            if entry.file_name().to_string_lossy().ends_with(".cdx.json") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    doc
}

fn crate_dirs(workspace_root: &Path) -> Result<Vec<PathBuf>> {
    let manifest: toml::Table = toml::from_str(
        &std::fs::read_to_string(workspace_root.join("Cargo.toml")).context("Cargo.toml")?,
    )?;
    Ok(manifest["workspace"]["members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m.as_str())
        .map(|m| workspace_root.join(m))
        .collect())
}

fn npm_bom(workspace_root: &Path) -> Result<Value> {
    let output = Command::new("npm")
        .args(["sbom", "--sbom-format", "cyclonedx", "--package-lock-only"])
        .current_dir(workspace_root.join(UI_DIR))
        .output()
        .context("cannot run `npm sbom` (npm 10 or later)")?;
    if !output.status.success() {
        bail!(
            "`npm sbom` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).context("`npm sbom` did not print JSON")
}

/// The release's version: the workspace's, which the shell and the engine both carry.
pub fn app_version(workspace_root: &Path) -> Result<String> {
    let manifest: toml::Table = toml::from_str(
        &std::fs::read_to_string(workspace_root.join("Cargo.toml")).context("Cargo.toml")?,
    )?;
    manifest["workspace"]["package"]["version"]
        .as_str()
        .map(str::to_owned)
        .context("no workspace version")
}

pub fn run(workspace_root: &Path, args: &[String]) -> Result<()> {
    let out = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .context("sbom --out <file.cdx.json> is required")?;
    let mut cargo = Vec::new();
    for (name, dir) in SHIPPED_CRATES {
        cargo.push(cargo_bom(workspace_root, name, dir)?);
    }
    let npm = npm_bom(workspace_root)?;
    let root = workspace_root.display().to_string();
    let bom = merge(
        &cargo,
        &npm,
        &natives(workspace_root)?,
        &app_version(workspace_root)?,
        &root,
    );
    let errors = validate(&bom, &workspace_root.join(SCHEMA_DIR))?;
    if !errors.is_empty() {
        bail!(
            "the SBOM is not valid CycloneDX {SPEC_VERSION}:\n{}",
            errors.join("\n")
        );
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out, serde_json::to_string_pretty(&bom)? + "\n")
        .with_context(|| format!("cannot write {}", out.display()))?;
    println!(
        "{}: CycloneDX {SPEC_VERSION}, {} components, valid",
        out.display(),
        bom["components"].as_array().map_or(0, Vec::len)
    );
    Ok(())
}
