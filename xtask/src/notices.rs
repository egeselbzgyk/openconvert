//! `cargo xtask notices` — the licence notices of every Rust crate the app and the engine are built
//! from, as one text file that ships in every bundle (PHASE 15 part B; LICENSE_AND_DEPENDENCIES §5).
//!
//! MIT, BSD, Zlib and Apache-2.0 all ask for the same thing in a binary distribution: the copyright
//! and licence text travel with the binary. `apps/desktop/ui/THIRD-PARTY-NOTICES.txt` does that for
//! the web view's code; this does it for the Rust side, from `cargo metadata`:
//!
//! - **Which crates:** everything reachable from `openconvert` and `openconvert-desktop` (default
//!   features) through normal and build dependencies, for *every* target a release ships — so one
//!   file serves all installers — and never a dev-dependency. The workspace's own crates are
//!   OpenConvert (LICENSE, NOTICE).
//! - **Which text:** every `LICENSE*`/`LICENCE*`/`COPYING*`/`NOTICE*`/`COPYRIGHT*` file the published
//!   crate carries, as published (line endings normalised). A crate that publishes none gets its
//!   authors as the copyright line and the standard text of the licence it is used under — the first
//!   of its alternatives in [`PREFERENCE`] — so no crate is listed without a text.
//! - **How:** crates sorted by name and version, identical texts printed once and referred to by
//!   number. The file is committed; `notices --check` and a test fail when it no longer matches
//!   `Cargo.lock`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

/// The committed notices file, relative to the workspace root.
pub const NOTICES_FILE: &str = "licenses/third-party-rust.txt";

/// The crates whose builds ship.
const SHIPPED: [&str; 2] = ["openconvert", "openconvert-desktop"];

/// File-name prefixes (upper-cased) that hold licence or notice text.
const LICENSE_PREFIXES: [&str; 6] = [
    "LICENSE",
    "LICENCE",
    "COPYING",
    "NOTICE",
    "COPYRIGHT",
    "UNLICENSE",
];

/// For a crate that publishes no licence file: which of its alternatives the notice uses.
const PREFERENCE: [&str; 5] = ["MIT", "Apache-2.0", "BSD-3-Clause", "Zlib", "MPL-2.0"];

/// The MIT licence, with the copyright line filled from the crate's authors.
const MIT_TEMPLATE: &str = "Copyright (c) {authors}

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the \"Software\"), to deal in the Software without restriction,
including without limitation the rights to use, copy, modify, merge, publish, distribute,
sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or
substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES
OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.";

/// The 3-clause BSD licence, likewise.
const BSD_3_TEMPLATE: &str = "Copyright (c) {authors}

Redistribution and use in source and binary forms, with or without modification, are permitted
provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions
   and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of
   conditions and the following disclaimer in the documentation and/or other materials provided
   with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to
   endorse or promote products derived from this software without specific prior written
   permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS \"AS IS\" AND ANY EXPRESS OR
IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND
FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER
IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT
OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.";

/// The zlib licence, likewise.
const ZLIB_TEMPLATE: &str = "Copyright (c) {authors}

This software is provided 'as-is', without any express or implied warranty. In no event will the
authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial
applications, and to alter it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the
   original software. If you use this software in a product, an acknowledgment in the product
   documentation would be appreciated but is not required.
2. Altered source versions must be plainly marked as such, and must not be misrepresented as being
   the original software.
3. This notice may not be removed or altered from any source distribution.";

/// One crate in the notices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrateNotice {
    pub name: String,
    pub version: String,
    pub license: String,
    /// The texts, as published or as supplied for a crate that publishes none.
    pub texts: Vec<String>,
}

/// `cargo metadata`, for the workspace at `root`.
pub fn metadata(root: &Path) -> Result<serde_json::Value> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(root)
        .output()
        .context("cannot run `cargo metadata`")?;
    if !output.status.success() {
        bail!(
            "`cargo metadata` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).context("`cargo metadata` did not print JSON")
}

/// A licence file's text as the notices print it: `\r\n` → `\n`, trailing space and blank lines
/// dropped, one final newline.
fn normalise(text: &str) -> String {
    let lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.trim_end_matches(['\r', ' ', '\t']))
        .collect();
    let mut out = lines.join("\n");
    while out.ends_with('\n') {
        out.pop();
    }
    out.trim_start_matches('\n').to_owned()
}

/// The licence files a crate publishes, in name order.
fn license_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("cannot list {}", dir.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| {
                    let upper = name.to_ascii_uppercase();
                    LICENSE_PREFIXES
                        .iter()
                        .any(|prefix| upper.starts_with(prefix))
                })
        })
        .collect();
    files.sort();
    Ok(files)
}

/// Which licence of an SPDX expression the notice uses for a crate without a licence file.
pub fn chosen_license(expression: &str) -> Option<&'static str> {
    let alternatives: BTreeSet<&str> = expression
        .split(|c: char| c == '/' || c.is_whitespace() || c == '(' || c == ')')
        .filter(|word| !word.is_empty() && *word != "OR" && *word != "AND")
        .collect();
    PREFERENCE
        .iter()
        .copied()
        .find(|preferred| alternatives.contains(preferred))
}

/// Every shipped crate's notice, sorted by name and version.
pub fn collect(root: &Path, metadata: &serde_json::Value) -> Result<Vec<CrateNotice>> {
    let packages: BTreeMap<&str, &serde_json::Value> = metadata["packages"]
        .as_array()
        .context("metadata has packages")?
        .iter()
        .filter_map(|p| p["id"].as_str().map(|id| (id, p)))
        .collect();
    let nodes: BTreeMap<&str, &serde_json::Value> = metadata["resolve"]["nodes"]
        .as_array()
        .context("metadata has a resolve graph")?
        .iter()
        .filter_map(|n| n["id"].as_str().map(|id| (id, n)))
        .collect();

    let mut stack: Vec<&str> = packages
        .iter()
        .filter(|(_, p)| {
            p["source"].is_null() && SHIPPED.contains(&p["name"].as_str().unwrap_or(""))
        })
        .map(|(id, _)| *id)
        .collect();
    if stack.len() != SHIPPED.len() {
        bail!("the workspace does not have both {SHIPPED:?}");
    }
    let mut reached = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !reached.insert(id) {
            continue;
        }
        let Some(node) = nodes.get(id) else { continue };
        for dep in node["deps"].as_array().into_iter().flatten() {
            let shipped = dep["dep_kinds"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|k| matches!(k["kind"].as_str(), None | Some("build")));
            if shipped {
                if let Some(pkg) = dep["pkg"].as_str() {
                    stack.push(pkg);
                }
            }
        }
    }

    let apache = std::fs::read_to_string(root.join("LICENSE")).context("the workspace LICENSE")?;
    let mut mpl: Option<String> = None;
    let mut notices = Vec::new();
    let mut without_files = Vec::new();
    for id in reached {
        let package = packages[id];
        if package["source"].is_null() {
            continue; // OpenConvert's own crates: LICENSE and NOTICE.
        }
        let name = package["name"].as_str().context("a name")?.to_owned();
        let version = package["version"].as_str().context("a version")?.to_owned();
        let license = package["license"].as_str().unwrap_or("").to_owned();
        let dir = Path::new(package["manifest_path"].as_str().context("a manifest")?)
            .parent()
            .context("a crate directory")?
            .to_path_buf();
        let mut texts = Vec::new();
        for file in license_files(&dir)? {
            let bytes = std::fs::read(&file)?;
            let text = normalise(&String::from_utf8_lossy(&bytes));
            if !text.is_empty() {
                if mpl.is_none() && text.starts_with("Mozilla Public License Version 2.0") {
                    mpl = Some(text.clone());
                }
                texts.push(text);
            }
        }
        let authors: Vec<&str> = package["authors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| a.as_str())
            .collect();
        if texts.is_empty() {
            without_files.push(notices.len());
        }
        notices.push((
            CrateNotice {
                name,
                version,
                license,
                texts,
            },
            authors.join(", "),
        ));
    }

    // Crates that publish no licence file: the standard text of the licence they are used under.
    for index in without_files {
        let (notice, authors) = &mut notices[index];
        let holders = if authors.is_empty() {
            format!("the {} authors", notice.name)
        } else {
            authors.clone()
        };
        let text = match chosen_license(&notice.license) {
            Some("MIT") => MIT_TEMPLATE.replace("{authors}", &holders),
            Some("BSD-3-Clause") => BSD_3_TEMPLATE.replace("{authors}", &holders),
            Some("Zlib") => ZLIB_TEMPLATE.replace("{authors}", &holders),
            Some("Apache-2.0") => format!("Copyright {holders}\n\n{}", normalise(&apache)),
            Some("MPL-2.0") => match &mpl {
                Some(text) => format!("Copyright {holders}\n\n{text}"),
                None => bail!("{} is MPL-2.0 and no crate carries its text", notice.name),
            },
            _ => bail!(
                "{} {} publishes no licence file and its licence `{}` has no standard text here",
                notice.name,
                notice.version,
                notice.license
            ),
        };
        notice.texts.push(text);
    }

    let mut out: Vec<CrateNotice> = notices.into_iter().map(|(n, _)| n).collect();
    out.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    Ok(out)
}

/// The notices file's text.
pub fn render(crates: &[CrateNotice]) -> String {
    let mut numbers: BTreeMap<&str, usize> = BTreeMap::new();
    let mut texts: Vec<(&str, Vec<String>)> = Vec::new();
    for notice in crates {
        for text in &notice.texts {
            let label = format!("{} {}", notice.name, notice.version);
            match numbers.get(text.as_str()) {
                Some(&n) => texts[n - 1].1.push(label),
                None => {
                    texts.push((text, vec![label]));
                    numbers.insert(text, texts.len());
                }
            }
        }
    }

    let mut out = String::new();
    out.push_str(
        "OpenConvert: third-party notices for the Rust code in the app and the converter\n\
         ================================================================================\n\n\
         Generated by `cargo run -p xtask -- notices` from Cargo.lock: every crate the converter\n\
         (`openconvert`) and the desktop app (`openconvert-desktop`) are built from, on every\n\
         platform a release ships for, their build-time dependencies included. Do not edit by hand.\n\
         OpenConvert's own licence is in LICENSE and NOTICE; the user interface's notices are shown\n\
         in the app (Settings, About, Licenses); PDFium's and llama.cpp's are beside this file.\n\n",
    );
    out.push_str(&format!(
        "{} crates, {} distinct licence texts.\n\nCrates\n------\n\n",
        crates.len(),
        texts.len()
    ));
    for notice in crates {
        let refs: Vec<String> = notice
            .texts
            .iter()
            .map(|t| format!("[{}]", numbers[t.as_str()]))
            .collect();
        out.push_str(&format!(
            "{} {} ({}) {}\n",
            notice.name,
            notice.version,
            if notice.license.is_empty() {
                "no licence field"
            } else {
                &notice.license
            },
            refs.join(" ")
        ));
    }
    out.push_str("\nTexts\n-----\n");
    for (index, (text, users)) in texts.iter().enumerate() {
        out.push_str(&format!(
            "\n[{}] {}\n{}\n\n{}\n",
            index + 1,
            users.join(", "),
            "-".repeat(72),
            text
        ));
    }
    out
}

pub fn run(root: &Path, args: &[String]) -> Result<()> {
    let text = render(&collect(root, &metadata(root)?)?);
    let path = root.join(NOTICES_FILE);
    if args.iter().any(|a| a == "--check") {
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        if committed != text {
            bail!(
                "{NOTICES_FILE} does not match Cargo.lock; run `cargo run -p xtask -- notices` and \
                 commit the result"
            );
        }
        println!("{NOTICES_FILE}: up to date");
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, &text).with_context(|| format!("cannot write {}", path.display()))?;
    println!("wrote {NOTICES_FILE} ({} bytes)", text.len());
    Ok(())
}
