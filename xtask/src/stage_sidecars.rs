//! `cargo xtask stage-sidecars` — put everything the app bundles where Tauri expects it
//! (PHASE 15 detail 1).
//!
//! The bundle has the same shape on every OS: the shell, the `openconvert` engine and
//! `llama-server` as `externalBin`, and beside them one directory of native libraries — PDFium and
//! the llama.cpp runtime the server loads — plus `models.toml`, `thresholds.toml` and the licences
//! of the natives as resources. The model, the OCR pack and the validation pack are never in it
//! (D12). This task stages all of that under `apps/desktop/src-tauri/bin/`:
//!
//! ```text
//! bin/openconvert-engine-<triple>[.exe]   the engine cargo built (externalBin)
//! bin/llama-server-<triple>[.exe]         the pinned llama.cpp server (externalBin, D8)
//! bin/native/                             libpdfium + the server's runtime libraries
//! bin/licenses/                           the natives' licence texts
//! bin/STAGE_STAMP                         what was staged, from where
//! ```
//!
//! Tauri's `externalBin` looks for `<name>-<target-triple>` beside the configured path, so the
//! binaries are copied under that name rather than symlinked or referenced in place. The per-OS
//! config files (`tauri.<os>.conf.json`) put `bin/native/` beside the binaries: `usr/bin` in the
//! AppImage, `Contents/MacOS` on macOS, the install directory on Windows. Beside is where the
//! engine already looks for PDFium, and where `llama-server` (`RUNPATH $ORIGIN`, `@loader_path`,
//! the Windows DLL search order) and ggml's backend loader look for theirs.
//!
//! A build stamp is written next to it (RT A5.7). The app refuses to start when the staged
//! engine's `hello.engine_version` differs from its own, and the stamp is what makes that
//! failure diagnosable rather than merely loud: it records which build was staged, when, and
//! from where.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::{fetch_llama_server, vendor_pdfium};

/// Where Tauri looks for the sidecar, relative to the workspace root.
pub const SIDECAR_DIR: &str = "apps/desktop/src-tauri/bin";

/// The engine binary cargo builds.
const ENGINE_NAME: &str = "openconvert";

/// The name the engine is staged under, before the triple suffix Tauri requires: the
/// `externalBin` entry in `tauri.conf.json` and `openconvert_desktop::engine::SIDECAR_NAME`.
///
/// Deliberately not [`ENGINE_NAME`]. `tauri-build` copies every `externalBin` into
/// `target/<profile>/` without its triple, and a sidecar called `openconvert` replaced the engine
/// cargo had just built there with whatever was last staged (PHASE 15 carry-over; the test is
/// `no_sidecar_shares_a_name_with_a_workspace_binary`).
pub const SIDECAR_NAME: &str = "openconvert-engine";

/// The model server's name, staged and bundled as upstream names it (D8).
pub const SERVER_NAME: &str = "llama-server";

/// The native libraries that live beside the binaries, under [`SIDECAR_DIR`].
pub const NATIVE_DIR: &str = "native";

/// The natives' licence texts, bundled as resources.
pub const LICENSES_DIR: &str = "licenses";

/// Records what was staged, so a mismatch report can say which build is on disk.
const STAMP_NAME: &str = "STAGE_STAMP";

/// The llama.cpp libraries `llama-server` needs at run time, by family: the server's own
/// implementation, what it links (`llama-common`, `llama`, `mtmd`, `ggml`, `ggml-base`), the
/// backends ggml loads beside it (`ggml-cpu` and its per-ISA variants, `ggml-metal`, `ggml-blas`)
/// and the OpenMP runtime the Windows CPU backend links.
///
/// Not the RPC backend: it exists to send tensors to another machine, and nothing on this app's
/// conversion path or model server may open a socket it does not own (D13.9). Not the other
/// tools' `*-impl` libraries either — the bundle carries the server, not the release.
const LLAMA_RUNTIME_FAMILIES: [&str; 11] = [
    "llama-server-impl",
    "llama-common",
    "llama",
    "mtmd",
    "ggml",
    "ggml-base",
    "ggml-cpu",
    "ggml-metal",
    "ggml-blas",
    "omp140",
    // Every `ggml-cpu-<isa>` variant; matched by prefix below.
    "ggml-cpu-",
];

/// The family a shared-library file name belongs to — `libggml-base.so.0.20.0`,
/// `libggml-base.0.20.0.dylib` and `ggml-base.dll` are all `ggml-base` — or `None` when the file is
/// not a shared library.
pub fn library_family(file_name: &str) -> Option<&str> {
    let is_library =
        file_name.ends_with(".dylib") || file_name.ends_with(".dll") || file_name.contains(".so");
    if !is_library {
        return None;
    }
    let bare = file_name.strip_prefix("lib").unwrap_or(file_name);
    bare.split('.').next().filter(|family| !family.is_empty())
}

/// Whether `file_name`, from a llama.cpp release, is part of what `llama-server` runs with.
pub fn is_llama_runtime(file_name: &str) -> bool {
    let Some(family) = library_family(file_name) else {
        return false;
    };
    LLAMA_RUNTIME_FAMILIES.iter().any(|wanted| {
        if let Some(prefix) = wanted.strip_suffix('-') {
            family
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('-') && rest.len() > 1)
        } else {
            family == *wanted
        }
    })
}

pub fn run(workspace_root: &Path, release: bool) -> Result<()> {
    let profile = if release { "release" } else { "debug" };
    let triple = env!("XTASK_HOST_TRIPLE");
    let extension = std::env::consts::EXE_SUFFIX;

    let built = workspace_root
        .join("target")
        .join(profile)
        .join(format!("{ENGINE_NAME}{extension}"));
    if !built.is_file() {
        bail!(
            "no engine at {}; run `cargo build -p {ENGINE_NAME}{}` first",
            built.display(),
            if release { " --release" } else { "" }
        );
    }

    let staged_dir = workspace_root.join(SIDECAR_DIR);
    std::fs::create_dir_all(&staged_dir)
        .with_context(|| format!("cannot create {}", staged_dir.display()))?;
    // What an older checkout staged under the engine's own name. Left in place it is inert, but
    // it is exactly the stale build this naming exists to keep away, so it goes.
    let legacy = staged_dir.join(format!("{ENGINE_NAME}-{triple}{extension}"));
    if legacy.is_file() {
        std::fs::remove_file(&legacy)
            .with_context(|| format!("cannot remove {}", legacy.display()))?;
    }
    let staged = staged_dir.join(format!("{SIDECAR_NAME}-{triple}{extension}"));
    copy(&built, &staged)?;

    // The version is asked of the binary rather than read from a manifest, so the stamp
    // describes what was actually staged.
    let version = std::process::Command::new(&staged)
        .arg("--version")
        .output()
        .with_context(|| format!("cannot run {}", staged.display()))?;
    let version = String::from_utf8_lossy(&version.stdout).trim().to_owned();

    let natives = stage_natives(workspace_root, &staged_dir, triple)?;

    let stamp = format!(
        "{version}\ntriple = {triple}\nprofile = {profile}\nsource = {}\nllama.cpp = {}\n\
         pdfium = {}\n",
        built.display().to_string().replace('\\', "/"),
        natives.llama_tag,
        natives.pdfium_build,
    );
    std::fs::write(staged_dir.join(STAMP_NAME), &stamp)
        .with_context(|| format!("cannot write {}", staged_dir.join(STAMP_NAME).display()))?;

    println!("staged {version} for {triple} at {}", staged.display());
    println!(
        "staged {SERVER_NAME} {} and {} native libraries in {}",
        natives.llama_tag,
        natives.libraries,
        staged_dir.join(NATIVE_DIR).display()
    );
    Ok(())
}

/// What [`stage_natives`] put in place.
struct Natives {
    llama_tag: String,
    pdfium_build: u32,
    libraries: usize,
}

/// `llama-server`, the native directory and the licences, from `vendor/`.
fn stage_natives(workspace_root: &Path, staged_dir: &Path, triple: &str) -> Result<Natives> {
    let extension = std::env::consts::EXE_SUFFIX;

    let lock = fetch_llama_server::lock()?;
    let llama_root = workspace_root.join("vendor/llama-server").join(&lock.tag);
    let Some(server) = fetch_llama_server::find_server(&llama_root) else {
        bail!(
            "no {SERVER_NAME} under {}; run `cargo run -p xtask -- fetch-llama-server` first",
            llama_root.display()
        );
    };
    let release_dir = server
        .parent()
        .context("llama-server has a parent directory")?
        .to_path_buf();
    copy(
        &server,
        &staged_dir.join(format!("{SERVER_NAME}-{triple}{extension}")),
    )?;

    let pdfium_dir = workspace_root.join("vendor/pdfium").join(triple);
    let pdfium = pdfium_dir.join(vendor_pdfium::library_file_name());
    if !pdfium.is_file() {
        bail!(
            "no PDFium at {}; run `cargo run -p xtask -- vendor-pdfium` first",
            pdfium.display()
        );
    }

    // Emptied first: a library left over from another llama.cpp pin would otherwise be bundled
    // beside a server that never asked for it.
    let native = staged_dir.join(NATIVE_DIR);
    if native.exists() {
        std::fs::remove_dir_all(&native)
            .with_context(|| format!("cannot empty {}", native.display()))?;
    }
    std::fs::create_dir_all(&native)
        .with_context(|| format!("cannot create {}", native.display()))?;
    copy(&pdfium, &native.join(vendor_pdfium::library_file_name()))?;

    let mut names: Vec<PathBuf> = std::fs::read_dir(&release_dir)
        .with_context(|| format!("cannot list {}", release_dir.display()))?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    names.sort();
    let mut libraries = 1;
    let mut has_server_impl = false;
    for path in names {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_llama_runtime(name) {
            continue;
        }
        has_server_impl |= library_family(name) == Some("llama-server-impl");
        copy_preserving_links(&path, &native.join(name))?;
        libraries += 1;
    }
    if !has_server_impl {
        bail!(
            "{} has no llama-server-impl library; the pin in xtask/llama.lock is not a build this \
             task knows how to bundle",
            release_dir.display()
        );
    }

    let licenses = staged_dir.join(LICENSES_DIR);
    std::fs::create_dir_all(&licenses)
        .with_context(|| format!("cannot create {}", licenses.display()))?;
    copy(
        &pdfium_dir.join("LICENSE"),
        &licenses.join("pdfium.LICENSE.txt"),
    )?;
    copy(
        &release_dir.join("LICENSE"),
        &licenses.join("llama.cpp.LICENSE.txt"),
    )?;

    Ok(Natives {
        llama_tag: lock.tag,
        pdfium_build: vendor_pdfium::pinned_build()?,
        libraries,
    })
}

fn copy(from: &Path, to: &Path) -> Result<()> {
    if to.exists() || to.is_symlink() {
        std::fs::remove_file(to).with_context(|| format!("cannot replace {}", to.display()))?;
    }
    std::fs::copy(from, to)
        .with_context(|| format!("cannot copy {} to {}", from.display(), to.display()))?;
    Ok(())
}

/// A symlink stays a symlink (`libllama.so.0 -> libllama.so.0.1.0`), so the bundle carries each
/// library once under every name the loader may ask for; Tauri's bundlers copy links as links.
fn copy_preserving_links(from: &Path, to: &Path) -> Result<()> {
    #[cfg(unix)]
    if from.is_symlink() {
        let target = std::fs::read_link(from)
            .with_context(|| format!("cannot read the link {}", from.display()))?;
        std::os::unix::fs::symlink(&target, to)
            .with_context(|| format!("cannot link {}", to.display()))?;
        return Ok(());
    }
    copy(from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The llama.cpp `b10456` release for Linux x64, macOS arm64 and Windows x64, as listed from
    /// the archives `xtask/llama.lock` pins (2026-09-23), less the tools' executables.
    const LINUX: &[&str] = &[
        "LICENSE",
        "ggml-rpc-server",
        "libggml-base.so",
        "libggml-base.so.0",
        "libggml-base.so.0.20.0",
        "libggml-cpu-alderlake.so",
        "libggml-cpu-haswell.so",
        "libggml-cpu-x64.so",
        "libggml-cpu-zen4.so",
        "libggml-rpc.so",
        "libggml.so",
        "libggml.so.0",
        "libggml.so.0.20.0",
        "libllama-bench-impl.so",
        "libllama-cli-impl.so",
        "libllama-common.so",
        "libllama-common.so.0",
        "libllama-common.so.0.1.0",
        "libllama-server-impl.so",
        "libllama.so",
        "libllama.so.0",
        "libllama.so.0.1.0",
        "libmtmd.so",
        "libmtmd.so.0",
        "libmtmd.so.0.1.0",
        "llama-server",
    ];
    const MACOS: &[&str] = &[
        "LICENSE",
        "libggml-base.0.20.0.dylib",
        "libggml-base.0.dylib",
        "libggml-blas.0.dylib",
        "libggml-cpu.0.20.0.dylib",
        "libggml-metal.0.dylib",
        "libggml-rpc.0.20.0.dylib",
        "libggml-rpc.dylib",
        "libllama-bench-impl.dylib",
        "libllama-common.0.dylib",
        "libllama-server-impl.dylib",
        "libllama.0.1.0.dylib",
        "libmtmd.0.dylib",
        "llama-server",
    ];
    const WINDOWS: &[&str] = &[
        "ggml-base.dll",
        "ggml-cpu-sse42.dll",
        "ggml-cpu-zen4.dll",
        "ggml-rpc-server.exe",
        "ggml-rpc.dll",
        "ggml.dll",
        "libomp140.x86_64.dll",
        "llama-bench-impl.dll",
        "llama-cli-impl.dll",
        "llama-common.dll",
        "llama-server-impl.dll",
        "llama-server.exe",
        "llama.dll",
        "mtmd.dll",
    ];

    fn runtime(files: &[&'static str]) -> Vec<&'static str> {
        files
            .iter()
            .copied()
            .filter(|f| is_llama_runtime(f))
            .collect()
    }

    #[test]
    fn the_llama_runtime_is_the_server_and_what_it_loads_on_every_os() {
        assert_eq!(
            runtime(LINUX),
            [
                "libggml-base.so",
                "libggml-base.so.0",
                "libggml-base.so.0.20.0",
                "libggml-cpu-alderlake.so",
                "libggml-cpu-haswell.so",
                "libggml-cpu-x64.so",
                "libggml-cpu-zen4.so",
                "libggml.so",
                "libggml.so.0",
                "libggml.so.0.20.0",
                "libllama-common.so",
                "libllama-common.so.0",
                "libllama-common.so.0.1.0",
                "libllama-server-impl.so",
                "libllama.so",
                "libllama.so.0",
                "libllama.so.0.1.0",
                "libmtmd.so",
                "libmtmd.so.0",
                "libmtmd.so.0.1.0",
            ]
        );
        assert_eq!(
            runtime(MACOS),
            [
                "libggml-base.0.20.0.dylib",
                "libggml-base.0.dylib",
                "libggml-blas.0.dylib",
                "libggml-cpu.0.20.0.dylib",
                "libggml-metal.0.dylib",
                "libllama-common.0.dylib",
                "libllama-server-impl.dylib",
                "libllama.0.1.0.dylib",
                "libmtmd.0.dylib",
            ]
        );
        assert_eq!(
            runtime(WINDOWS),
            [
                "ggml-base.dll",
                "ggml-cpu-sse42.dll",
                "ggml-cpu-zen4.dll",
                "ggml.dll",
                "libomp140.x86_64.dll",
                "llama-common.dll",
                "llama-server-impl.dll",
                "llama.dll",
                "mtmd.dll",
            ]
        );
    }

    #[test]
    fn the_rpc_backend_and_other_tools_are_never_bundled() {
        for file in LINUX.iter().chain(MACOS).chain(WINDOWS) {
            if file.contains("rpc") || file.contains("bench") || file.contains("cli-impl") {
                assert!(!is_llama_runtime(file), "{file}");
            }
        }
        for not_a_library in ["LICENSE", "llama-server", "llama-server.exe", "ggml-cpu-"] {
            assert!(!is_llama_runtime(not_a_library), "{not_a_library}");
        }
    }
}
