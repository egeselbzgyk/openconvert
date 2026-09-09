# Decisions log

Append-only. One entry per spike result, verification-debt closure, or small implementation decision that is
not already settled in `DECISIONS.md`. Newest last.

Format:

```
## YYYY-MM-DD · <short title> · <phase>
Context: ...
Decision: ...
Evidence: <command output, file, URL, or test name>
Affects: <D-id / doc section / crate>
```

<!-- Verification debt (Phase 0 table VD-a…VD-g) closes here, each with the source actually consulted. -->

## 2026-09-09 · VD-a closed: `zip` major version confirmed as 8.x · Phase 0
Context: Phase 0 acceptance A0.9 requires VD-a (TECHNOLOGY_EVALUATION §14.1 / V2 §9) to close before
`zip` is pinned in the workspace manifest — V2 reported 8.6.0 from a single source and flagged the jump
from the previously assumed 0.42 for manual spot-check.
Decision: pin `zip = "8.6"` as the plan specifies (§1.2). No change.
Evidence: crates.io sparse index `https://index.crates.io/3/z/zip`, read 2026-09-09. Full stable
release order ends `… 8.4.0, 8.5.0, 8.5.1, 8.6.0`; the only higher versions are `9.0.0-pre1/-pre2/-pre3`,
which are pre-releases and not selectable by a `"8.6"` requirement. 8.6.0 is therefore the current
stable major, and V2's number is confirmed against the registry itself.
Affects: IMPLEMENTATION_PLAN §1.2, Phase 0 VD-a, Phase 5 (`oc-epub` deterministic zip).

## 2026-09-09 · `pdfium-render` 0.9.4 feature names corrected · Phase 0
Context: IMPLEMENTATION_PLAN §1.2 pins `pdfium-render = { version = "0.9.4", default-features = false,
features = ["image", "libloading"] }`. Neither feature exists; `cargo generate-lockfile` refuses the
manifest ("`pdfium-render` does not have these features").
Decision: use `features = ["pdfium_latest", "image_025", "thread_safe"]`.
Evidence: the crates.io index entry for `pdfium-render` 0.9.4 declares `default = ["pdfium_latest",
"image_latest", "thread_safe"]`, `image_025 = ["dep:image_025", "image_api"]`, and lists `libloading ^0`
as a *non-optional* dependency for `cfg(not(target_arch = "wasm32"))`. Runtime binding via
`Pdfium::bind_to_library` (D3) therefore needs no feature at all, and the image API is reached through
`image_025`, which matches the workspace `image = "0.25"` pin. `pdfium_latest` will be replaced by the
exact `pdfium_<build>` feature matching the vendored `bblanchon/pdfium-binaries` release when
`xtask vendor-pdfium` pins it (Phase 0 implementation detail 1).
Affects: D3, IMPLEMENTATION_PLAN §1.2, `crates/oc-pdf`.

## 2026-09-09 · Pinned Rust toolchain moved 1.85.0 → 1.88.0 · Phase 0
Context: IMPLEMENTATION_PLAN §1.3 pins `channel = "1.85.0"`, with the stated rationale that *pinning*
(not that particular number) keeps `clippy -D warnings` stable. The plan's own V2-verified dependency
pins cannot be built by 1.85.0.
Decision: pin `1.88.0` — the lowest toolchain that builds the dependency set §1.2 specifies — in
`rust-toolchain.toml`, `[workspace.package] rust-version`, and the three `ci.yml` toolchain steps.
Evidence: `cargo generate-lockfile` on the §1.2 dependency set reports MSRVs above 1.85 for
`zip 8.6.0` (1.88), `lopdf 0.45.0` (1.88), `image 0.25.10` (1.88), `libloading 0.9.0` (1.88),
`time 0.3.55` (1.88), `weezl 0.2.1` (1.88), `wasip2` (1.87), `quick-xml 0.42.0` (1.86),
`multiversion 0.9.0` (1.86); the maximum is 1.88.0.
Affects: IMPLEMENTATION_PLAN §1.2/§1.3/§1.9. DECISIONS.md does not name a Rust version (D1), so no ADR
decision is contradicted.

## 2026-09-09 · RT B1 substitution recorded: no `has_unicode_map_error()` · Phase 0
Context: D3's residual-risk note and Phase 0 implementation detail 3 require a one-paragraph note here
recording what replaces `has_unicode_map_error()`, which V2 could not find in `pdfium-render`.
Decision: `BrokenText` classification never calls it. Phase 0 detects broken CMaps from the
U+FFFD + PUA share of visible characters against `pageclass.broken_text_replacement_share` (0.20);
Phase 2 adds the second arm, dictionary hit rate against `pageclass.broken_text_dict_hit_min` (0.35),
which `classify_page` already accepts as `Option<f32>` and Phase 0 passes as `None`.
Evidence: V2 §1 (API-name verification), RT B1, IMPLEMENTATION_PLAN Phase 0 detail 3 and failure-mode
table. To be re-confirmed against the vendored PDFium headers when `xtask vendor-pdfium` lands.
Affects: D3, `crates/oc-pdf/src/classify.rs`, thresholds `pageclass.broken_text_*`.

## 2026-09-09 · `webpki-roots` licence blocks `ureq` under D15's allow-list · Phase 0 (flagged for Phase 9)
Context: bootstrapping `oc-net` with `ureq = { version = "3", features = ["rustls"] }` (§1.2) makes
`cargo deny check licenses` fail: `webpki-roots 1.0.9` is `CDLA-Permissive-2.0`, which is not on D15's
allow-list (MIT, Apache-2.0, BSD-2/3, ISC, MPL-2.0, Unicode, Zlib, CC0).
Decision: Phase 0 does not implement `oc-net` (Phase 0 scope excludes all network code), so the `ureq`
dependency is not declared yet; the manifest carries a comment pointing here. `cargo deny check` is
clean at bootstrap. The choice is deferred to Phase 9, which owns model downloads, and is one of:
(a) source TLS roots from the platform verifier / `rustls-native-certs` instead of `webpki-roots`;
(b) a `deny.toml` `[licenses] exceptions` entry scoped to `webpki-roots` with an issue link;
(c) amend D15's allow-list with `CDLA-Permissive-2.0` — an ADR change, which would mean stopping and
asking rather than deciding in-phase.
Evidence: `cargo deny check licenses` on the bootstrap graph:
`error[rejected] webpki-roots-1.0.9/Cargo.toml:26 license = "CDLA-Permissive-2.0" — not explicitly allowed`.
Affects: D15, IMPLEMENTATION_PLAN §1.2/§1.4, `crates/oc-net`, Phase 9.

## 2026-09-09 · Intra-workspace path dependencies carry explicit versions · Phase 0
Context: `deny.toml` sets `wildcards = "deny"` (§1.4). A `{ path = "../oc-model" }` dependency with no
version requirement is a wildcard, so `cargo deny check bans` failed on all eleven internal crates.
Decision: every internal path dependency is written `{ path = "…", version = "0.1.0" }`.
Evidence: `cargo deny check bans` — `error[wildcard]: found 1 wildcard dependency for crate 'oc-core'`
before the change; `bans ok` after.
Affects: IMPLEMENTATION_PLAN §1.4, every `crates/*/Cargo.toml`.

## 2026-09-09 · `BlockId` derivation: the details D13.3 leaves open · Phase 0
Context: D13.3 fixes the id as `base32(blake3(page_index ‖ bbox rounded to 1 pt ‖ first 64 NFC chars))[..10]`
with a collision suffix, and `IR_SKETCH.md` fixes the type as `[u8; 10]`. Four things had to be pinned
before the first id could be computed, none of them settled by the ADR.
Decision:
1. **Field encoding.** `page_index` as `u32::to_le_bytes`; each bbox coordinate rounded with `f32::round`,
   cast to `i32` (saturating, so a non-finite coordinate cannot panic), then `i32::to_le_bytes`. The three
   fixed-width fields come first, so the variable-width text needs no separator to keep the concatenation
   unambiguous. Little-endian is chosen explicitly rather than inherited from the host, so ids are
   identical on every target (D13.8's determinism contract).
2. **Text prefix in characters, not bytes.** `text.nfc().take(64)`, so the truncation point does not move
   with the script. `derive` truncates; callers pass the whole block text.
3. **Alphabet.** RFC 4648 base32, unpadded, uppercase. Note for Phase 5: an id may begin with a digit,
   so an XHTML `id` attribute built from one needs a prefix — an XML `NCName` cannot start with a digit.
4. **The collision suffix is the tenth character, not an eleventh.** `[u8; 10]` leaves no room to append,
   so the counter occupies the last character and the digest supplies the first nine (45 bits, plus a
   5-bit counter). `derive` always emits counter 0; `with_collision_suffix(n)` writes `n % 32`. This makes
   distinctness within a collision family an *invariant* — every variant differs from the base and from
   every other variant in a known position — rather than a probability, which is what lets test 0.2 assert
   it deterministically over 10 000 generated triples instead of relying on 50-bit luck. The cost is 45
   rather than 50 bits of base entropy: ~1.4e-6 chance of one collision in a 10 000-block book, which is
   exactly the case the suffix exists to resolve.
Evidence: `cargo nextest run -p oc-model` — `ids::block_id_is_stable_for_same_inputs` (golden id
`SDMLH752SA` for `derive(3, Rect{72.0, 96.5, 340.25, 118.0}, "Chapter 3")`) and
`ids::prop_block_id_collision_suffix_is_unique` (10 000 cases) both pass.
Affects: D13.3, `IR_VERSION`, `crates/oc-model/src/ids.rs`, Phase 5 (XHTML id emission).

## 2026-09-09 · Format constants live in code, not in `thresholds.toml` · Phase 0
Context: CLAUDE.md forbids numeric literals in production code, sourcing every constant from
`thresholds.toml`. `ids.rs` needs 10 (id length), 9 (hash characters), 64 (text prefix) and 32 (alphabet
size).
Decision: these are named `const` items in the module, not threshold entries.
Evidence: D17 defines `thresholds.toml` as the home of *tunable* numbers — every entry carries
`source`/`evidence`/`owner`/`review_by` and is subject to `eval calibrate`. The id format constants are
none of those things: changing one changes `ir_version` and invalidates every cache and override, so it is
a versioned format change, not a calibration. The plan itself puts `pub const IR_VERSION: u32 = 1` in
`oc-model`, not in `thresholds.toml`, which is the same category. The rule is honoured in substance: no
magic numbers, every constant named and documented against the decision that fixes it.
Affects: CLAUDE.md §2 hard rules, `crates/oc-model/src/ids.rs`, `xtask thresholds-lint` scope.

## 2026-09-09 · Canonical JSON: the rounding rule, the compact form, and a bespoke serialiser · Phase 0
Context: ARCHITECTURE §4.3 fixes the contract — keys sorted, `ir_version` first, arrays in document order,
strings NFC, no NaN/Inf ("a serialization error, not a silent `null`"), "`f32` geometry printed with **2
decimals at serialization only**". Three things had to be decided to implement it.
Decision:
1. **The rounding rule is by type: `f32` prints with two decimals, `f64` prints shortest-round-trip.**
   A generic serialiser cannot tell a geometric float from any other one, and threading
   `#[serde(serialize_with = ...)]` through every coordinate field in the IR would be both invasive and
   easy to forget on a new field. §4.3's own wording is "`f32` geometry", and the IR uses `f32` for
   geometry and confidences throughout while `f64` is reserved for ratios and statistics — so the type
   *is* the discriminator, and the rule is enforced by construction rather than by remembering an
   attribute. Confidences round to two decimals as well, which is more precision than they carry.
   In-memory values are untouched (RT B5).
2. **Compact output**, no insignificant whitespace, no trailing newline, non-ASCII written literally as
   UTF-8. The canonical form exists to be byte-identical for identical input (D13.8) and to be hashed;
   indentation is presentation, and `--dump-stage` can pretty-print separately if that is ever wanted.
   The plan's own expected output for test 0.3, `"x0":1.23`, has no space after the colon.
3. **A bespoke `serde::Serializer` rather than `serde_json`.** `serde_json` writes a non-finite float as
   `null` — it checks `is_finite()` and calls `Formatter::write_null`, so no formatter hook can tell a
   NaN from a real `None` afterwards. A silent `null` where a coordinate belongs is exactly the failure
   §4.3 rules out. Key ordering is also not expressible in a streaming formatter, so the value tree is
   built first and written second, which the NaN check needs anyway.
Evidence: `cargo nextest run -p oc-model` — 4 tests pass; the committed snapshot
`oc_model__canonical__canonical_json_sorts_keys_and_rounds_geometry.snap` shows `f32` `1.234567 -> 1.23`
beside `f64` `0.126 -> 0.126`, `ir_version` first, sorted keys at both levels, `cafe`+U+0301 emitted as
NFC `café`, and `None` as `null`.
Affects: D13.3, D13.8, ARCHITECTURE §4.3, `crates/oc-model/src/canonical.rs`, every future IR type.

## 2026-09-09 · Threshold codegen: OUT_DIR, uniform types, and where the expiry rule lives · Phase 0
Context: §1.5 says "`xtask` generates `crates/oc-core/src/thresholds_generated.rs` from this file at build
time (a `build.rs` in `oc-core` reading `../../thresholds.toml`)", and Phase 0 detail 6 says the build
script "fails the build on a malformed entry" while "`xtask thresholds-lint` additionally enforces
owner/expiry". Four things needed deciding.
Decision:
1. **The generated file goes to `OUT_DIR` and is pulled in with `include!`**, not written into `src/`.
   A build script that writes into its own source tree breaks read-only checkouts, vendored builds and
   `cargo package`, and Cargo warns about it. Nothing is lost: the text is inspectable at
   `target/<profile>/build/oc-core-*/out/thresholds_generated.rs`, and it is regenerated whenever
   `thresholds.toml` changes (`cargo:rerun-if-changed`).
2. **Uniform value types: TOML float -> `f64`, integer -> `i64`, boolean -> `bool`.** The alternative,
   picking `u64` when a value happens to be non-negative, makes the *type* depend on the *value*, so
   changing a number from 0 to -1 would silently break every caller. Uniform typing costs a cast at some
   call sites and cannot surprise anyone.
3. **The expiry rule is a lint, not a build failure.** An expired `review_by` must fail CI (D17), but
   making it fail `cargo build` means the workspace stops compiling on a date nobody touched — including
   on an old commit someone is bisecting. So `build.rs` rejects only *malformed* entries (missing or
   misspelled `source`, a non-scalar `value`, a key that is not a Rust field name, a wrong
   `schema_version`), and the owner/expiry rule lives in `oc_core::thresholds::lint`, asserted by test 0.5
   and reused by `xtask thresholds-lint`. One implementation of the rule, two callers.
   `lint` takes `today` as an ISO string rather than reading a clock, so it is a pure function; ISO dates
   compare correctly as strings, so no date type is needed in `oc-core` at all — `time` is a
   *dev*-dependency, used only by the test to ask what day it is.
4. **`model_gate.g4_max_seconds_on_L` renamed to `..._on_l`.** The build script generates a struct field
   per key, and a field with a capital letter trips `non_snake_case`, which `-D warnings` makes an error.
   §1.5 wrote the key with D9's machine name "L" capitalised; the codegen requirement wins and the
   machine is named in a comment on the key instead. Value, source, evidence and owner are unchanged.
Evidence: `cargo nextest run -p oc-core` — `thresholds::every_provisional_has_owner_and_future_review`
(all 79 entries pass, and four hand-written fixtures prove the lint still fires on a missing owner, an
expired date, a missing date, and not on a `published` entry) and `thresholds::generated_constants_match_toml`
(`T.layout.furniture.band_ratio == 0.08` and equals the runtime-parsed value; `PROVENANCE.len()` equals
the number of entries in the file). The capital-L key was caught by the build script itself.
Affects: D17, IMPLEMENTATION_PLAN §1.5 and Phase 0 detail 6, `thresholds.toml`, `crates/oc-core`.

## 2026-09-09 · Page-space normalisation: a separate source type, and one extra test · Phase 0
Context: Phase 0 detail 5 requires the normalised space to be computed once — origin top-left, y down,
points, after `/Rotate` and after the CropBox offset — with a `debug_assert!` that every produced rect
lies inside the page box inflated by 1 pt, because R1 §D.6 #1 documents this exact mix-up silently
deleting body text in a shipping 2026 tool. Test 0.8 is the property.
Decision:
1. **PDF user space gets its own type, `oc_pdf::geom::PdfRect`**, rather than reusing `oc_model::Rect`.
   The failure mode the invariant exists to prevent is *passing a rectangle from one space to a function
   expecting the other*. An assertion catches that only when the numbers happen to fall outside the page;
   a distinct type catches it at compile time, everywhere, for free. `PageGeometry::normalise` is then the
   single doorway between the two spaces.
2. **Both corners are mapped and then re-ordered.** Two of the four rotations move the lower-left corner
   to somewhere that is no longer lower-left, so taking the mapped corners as `(x0,y0)`/`(x1,y1)` yields
   inverted rectangles under 180° and 270°.
3. **A unit test was added beyond the plan's table: `geom::normalises_corners_for_each_rotation`.**
   Test 0.8 asserts that a mapped rect lands inside the page box — but a rotation applied in the *wrong
   direction* also lands inside the page box, so the property as specified cannot detect a reversed or
   transposed rotation at all. The unit test pins where the crop box's bottom-left corner ends up under
   each of the four rotations, on a landscape page whose origin is not (0,0) so a forgotten offset fails
   too. The property keeps its job: it covers offsets and sizes the unit test cannot enumerate.
4. **`INSIDE_PAGE_TOLERANCE_PT = 1.0` is a code constant, not a threshold** — same reasoning as the id
   format constants: it is a float-error epsilon in an assertion, not a tunable policy number, and D17's
   file is for numbers that carry an owner and a review date.
Evidence: `cargo nextest run -p oc-pdf` — 2 passed; `PROPTEST_CASES=4096` also passes. The property found
one real defect on first run, in the test rather than the code: comparing the page size against the
generated `crop_width` fails in `f32` because `(llx + crop_width) - llx` is not `crop_width` once the
offset is large next to the page. The expected size now comes from the crop box itself, so the comparison
is exact. The failing seed is committed in `crates/oc-pdf/proptest-regressions/geom.txt`.
Affects: D13.3, RT D10, IMPLEMENTATION_PLAN Phase 0 detail 5 and test 0.8, `crates/oc-pdf/src/geom.rs`.

## 2026-09-09 · PDFium pinned to chromium/7881, vendored via curl+tar, probed behaviourally · Phase 0
Context: Phase 0 detail 1 requires `xtask vendor-pdfium` to fetch the `bblanchon/pdfium-binaries` asset
for the host triple against a SHA-256 pinned in `xtask/pdfium.lock`, and detail 3 / D3 require a startup
ABI probe. Several details were open.
Decision:
1. **Pin `chromium/7881`, not the newest release.** The newest is `chromium/8044` (2026-09-07), but
   `pdfium-render` 0.9.4's highest declared binding set is `pdfium_7881` — its `pdfium_latest` feature
   *is* `pdfium_7881`. Vendoring 8044 against 7881 bindings is precisely the ABI mismatch D3 lists as a
   residual risk, for no benefit. The workspace manifest now selects `pdfium_7881` explicitly rather
   than `pdfium_latest`, so a `pdfium-render` upgrade cannot silently move the target build.
2. **The build number is pinned in three places and cross-checked.** `xtask/pdfium.lock` (`build = 7881`),
   the `pdfium_7881` cargo feature, and `oc_pdf::pdfium::EXPECTED_PDFIUM_BUILD`. `vendor-pdfium` refuses
   to run when the manifest and the lock disagree, and also checks the `VERSION` file inside the archive;
   test 0.7 checks the constant against the lock and against the library it actually bound. The check
   earned itself immediately: the first run failed because the manifest still said `pdfium_latest`.
3. **Download and extraction shell out to `curl` and `tar`.** Both ship with Windows 10+, macOS and every
   CI runner image. This keeps D13.9's dependency firewall trivially true — no crate outside `oc-net`
   gains an HTTP client, not even a build-time one — and avoids adding `flate2`/`tar` to the tree.
4. **The ABI probe is behavioural, not a version symbol.** PDFium exports no version function, and
   `has_unicode_map_error()` does not exist in `pdfium-render` (RT B1). So `bind()` opens a 437-byte
   hand-built one-page PDF (`crates/oc-pdf/src/pdfium/probe.pdf`, verified against pypdfium2) through the
   loaded library and requires exactly one page, then compares the `VERSION` file beside the library to
   `EXPECTED_PDFIUM_BUILD`. A library supplied through `OC_PDFIUM_PATH` with no `VERSION` beside it gets
   the behavioural half only, which is all that can be checked.
5. **`OC_PDFIUM_PATH` is authoritative.** When it is set, no other candidate is tried. An override that
   silently falls back to the vendored copy is worse than one that fails loudly, and asserting the
   failure is also what proves test 0.7 is not passing vacuously.
Evidence: `cargo run -p xtask -- vendor-pdfium` fetched `pdfium-win-x64.tgz` and reported
`vendored pdfium 151.0.7881.0 (chromium/7881)`. The pinned SHA-256s came from the GitHub release API and
`73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08` was independently re-computed from the
downloaded archive with `sha256sum`. `cargo nextest run -p oc-pdf` — 3 passed.
Affects: D3, D13.9, RT B1, IMPLEMENTATION_PLAN §1.2 and Phase 0 details 1 and 3, `xtask`, `crates/oc-pdf`.

## 2026-09-09 · Binary fixtures are marked `binary` in `.gitattributes` · Phase 0
Context: the repo's `.gitattributes` says `* text=auto`. Git classifies a file as text unless it finds a
NUL byte in the first 8 KiB. `crates/oc-pdf/src/pdfium/probe.pdf` is 437 bytes of mostly-ASCII PDF with
no NUL, so git classified it as text and warned that it would rewrite its newlines on the next checkout.
Decision: `*.pdf`, `*.png`, `*.jpg`, `*.jpeg`, `*.gguf`, `*.epub`, `*.tgz`, `*.zip`, `*.ttf`, `*.otf` are
declared `binary`. This matters well beyond the probe: Phase 0 commits `corpus/fixtures/assets/*.png`,
Phase 0/4 commit hand-made PDFs, and Phase 5 compares EPUBs byte for byte (D13.8).
Evidence: `git check-attr` now reports `text: unset, binary: set` for the probe. A fresh `git clone` of
the repository produces a byte-identical `probe.pdf`
(`7b40d7f0920d9fe4c1b92cd620ef3a77e4f3b8fb0b65841148685eba8dc23d75`), which it would not have done on a
Windows checkout before the change — every offset in the PDF's cross-reference table would have shifted.
Affects: `.gitattributes`, IMPLEMENTATION_PLAN §0.6, every committed fixture from here on.

## 2026-09-09 · Page-class confidences moved into `thresholds.toml`; no geometry parameter · Phase 0
Context: Phase 0 detail 3 states the classification confidences inline (0.95, 0.9, 0.9, 0.8, 0.7, 1.0,
0.5) and gives the signature `classify_page(g: &PageGeometry, c: &PageCharStats, i: &PageImageStats,
dict_hit_rate: Option<f32>, t: &Thresholds)`.
Decision:
1. **The seven confidences are new `pageclass.confidence.*` entries in `thresholds.toml`**, all
   `source = "provisional"`. CLAUDE.md's hard rule is that no numeric literal appears in production code
   and every constant comes from that file, and unlike the id-format constants these really are the kind
   of number `eval calibrate` should later fit — a confidence is a claim about how often the verdict is
   right, which is measurable per producer stratum (D17, D18). `fallback_blank` is named separately from
   `blank` because the two share a class but not a claim: 0.9 for a page with nothing on it, 0.5 for the
   `otherwise` arm, which exists so the report can flag it.
2. **`classify_page` takes no `PageGeometry`.** None of the seven arms reads geometry — detail 3 itself
   calls it "a pure function over the counters" — and §0.2 forbids unused parameters. The image share is
   already normalised to `[0, 1]` by the caller that computed it, so page size never enters here. If a
   later arm needs geometry, adding the parameter is a one-line change at the two call sites.
3. **A fifth test was added: `classify::classify_mixed_blank_and_dictionary_arm`.** Tests 0.9–0.12 cover
   four of the seven arms; `mixed`, the clear `blank`, the `otherwise` fallback and the dictionary-hit
   arm are untouched by them. The dictionary arm matters most: Phase 0 always passes `None`, so without
   a test it would be dead wiring until Phase 2 and any mistake in it would surface there rather than
   here.
Evidence: `cargo nextest run -p oc-pdf` — 8 passed. `cargo run -p xtask -- thresholds-lint` is not yet
implemented, but `oc_core::thresholds::lint` (test 0.5) covers the seven new entries and passes.
Affects: D13.10, D17, IMPLEMENTATION_PLAN Phase 0 detail 3 and tests 0.9–0.12, `thresholds.toml`,
`crates/oc-pdf/src/classify.rs`.

## 2026-09-09 · Producer detection takes two strings, not an `InfoDict` · Phase 0
Context: Phase 0's architecture block gives `producer_family(info: &InfoDict, xmp: Option<&XmpMeta>)`,
and detail 4 gives the ordered regex table. Neither `InfoDict` nor `XmpMeta` exists yet — they are the
`lopdf`-backed types Phase 1 introduces.
Decision:
1. **The signature is `producer_family(producer: Option<&str>, creator: Option<&str>)`.** Detection reads
   exactly two strings; whether they came from the `/Info` dictionary or from XMP is the caller's
   business, and inventing both types now to hold two optional strings would be the speculative
   generality §0.2 rules out. When Phase 1 adds `InfoDict`, it passes its fields in.
2. **`/Producer` first, `/Creator` only as a fallback, never as an override.** `/Producer` names the tool
   that wrote the bytes and `/Creator` the application the document came from, so a recognised producer
   is the stronger signal; but generic and empty producers are common enough that ignoring the creator
   would lose real strata.
3. **`RegexSet`, taking the lowest matching index.** That is precisely "the first rule in the table
   wins", in one pass over the string, without compiling eight regexes per call — the set is built once
   in a `OnceLock`.
4. **`PdfTeX` serialises as `"pdfTeX"`** to match D18's stratum spelling; every other variant serialises
   as written. The two vocabularies are related but not identical: `corpus/manifest.json`'s
   `producer_stratum` also has `Quark` (which we detect as `Unknown`) and writes our own renderers as
   `ours(Typst)` / `ours(WeasyPrint)`, because a stratum records provenance while `ProducerFamily`
   records detection. Phase 7 owns the mapping between them.
5. **The test asserts its own completeness.** `ProducerFamily::ALL` exists so the table-driven test can
   check it covers every variant; adding a variant without a case fails test 0.13 rather than going
   untested.
Evidence: `cargo nextest run -p oc-pdf` — 9 passed. The table covers all nine variants plus three
`/Creator` precedence cases, the `^typst` anchor (a string that mentions Typst is not Typst, but leading
whitespace does not defeat it), and the two serialised spellings.
Affects: D13.10, D18, IMPLEMENTATION_PLAN Phase 0 detail 4 and test 0.13,
`crates/oc-pdf/src/producer.rs`, Phase 1 (`InfoDict`), Phase 7 (stratum mapping).

## 2026-09-09 · Q1 resolved: the advisory gate is scoped to what ships · Phase 0
Context: `DECISIONS.md` Appendix A and `TEST_CORPUS.md` §6.1 require Typst fixtures compiled in-process
via the `typst` + `typst-pdf` crates. Adding them takes the lockfile from 233 to 447 crates and makes
`cargo deny check` report seven advisories — two of them live vulnerabilities in `quick-xml` 0.38.4
(RUSTSEC-2026-0194 quadratic attribute parsing, RUSTSEC-2026-0195 unbounded namespace allocation),
reached through `citationberg` → `hayagriva` → `typst-library`, with no in-range fix because
`citationberg` 0.7.0 requires `^0.38` while the fix landed in 0.41. The other five are unmaintained
notices: `rustybuzz`, `ttf-parser` (via `krilla` → `typst-pdf`), `bincode`, `yaml-rust` (via `syntect` →
`two-face`), `paste` (via `biblatex`). Meanwhile §1.4 sets `ignore = []` and A0.2 requires the gate to
pass. Both cannot hold. Ruled by the maintainer: solve it without changing how the program works, and
solve it so it does not recur.
Decision: **split the audit surface along the line that actually matters — what ships versus what
builds — rather than accumulating per-advisory exceptions.**
- `deny.toml` gains `[graph] exclude = ["xtask"]` and keeps every rule absolute: `exceptions = []`,
  `ignore = []`, advisories enforced. It audits the shipped crates and nothing else. Excluding a crate
  drops only dependencies nothing else needs, so everything `xtask` shares with a shipped crate is still
  audited there.
- New `deny.tools.toml` audits `xtask`'s graph with the **same licence allow-list** (a build tool is not
  a reason to accept a licence the project would refuse elsewhere) and the same ban and source rules,
  because those are obligations of the repository rather than of the binary: a GPL build tool would
  still make this repo's Apache-2.0 licence a lie, and an unexpected git source is an attack on the
  maintainer's machine whether or not it ships. Advisories there are reported on every build as a
  `continue-on-error` CI step, not enforced.
- `docs/SECURITY.md` §9 now states the scope of each gate, so the document and CI agree.
Why this and not the alternatives: a per-id `ignore` list is precisely the thing that recurs — every
Typst release reshuffles a tree of 214 crates we do not control, and each reshuffle would mean another
exception and another review date. Moving `xtask` out of the workspace (D14 says one workspace) would
hide the finding behind repo layout instead of accepting it, and would leave the tooling tree unaudited
for licences too, which is worse. Committing the fixture PDFs as golden binaries would give up
`cargo xtask fixtures`, which TEST_CORPUS §6.1 ratifies and which Phase 7's mutation fixtures build on.
The threat-model argument is real and not a fig leaf: `xtask` has no user, no untrusted input — it reads
`.typ` sources and a `pdfium.lock` this repository authors — and is never linked into the engine, the
CLI or the desktop app. A denial-of-service parsing bug there has no attacker and no victim.
Evidence: `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok, with the Typst crates
present in the workspace. `cargo deny --config deny.tools.toml check licenses bans sources` — bans ok,
licenses ok, sources ok. `cargo deny --config deny.tools.toml check advisories` — FAILED with the seven
findings above, reported by the non-blocking CI step.
Affects: D14, D15, D18, SECURITY.md §9, IMPLEMENTATION_PLAN §1.2 and §1.4, `deny.toml`,
`deny.tools.toml`, `.github/workflows/ci.yml`, `xtask`.

## 2026-09-09 · Fixtures: tagging is turned off at export, not stripped afterwards · Phase 0
Context: Phase 0's fixture recipe is `typst::compile` → `typst_pdf::pdf` → "`strip_structtree` over the
`lopdf` document — remove `/StructTreeRoot`, `/MarkInfo` and marked-content wrappers by default" (D18:
Typst tags PDFs by default while the real world is 12.6 % tagged).
Decision: **`PdfOptions { tagged: false }`**, so no structure tree is ever produced. No `lopdf`
post-processing step exists. The end state D18 asks for is reached more completely: `/StructTreeRoot` and
`/MarkInfo` are absent (verified by byte search), and — unlike stripping — the marked-content operators
inside the content streams are absent too, which the plan's own wording admits a stripper would have to
chase separately. The tagged bucket is the same call with `tagged: true`, written as
`<fixture>__tagged.pdf`, and `/StructTreeRoot` is present in those.
Two further details the plan leaves open:
- **The Typst project root is `corpus/fixtures`, and the source is compiled at the virtual path
  `typst/main.typ`.** The fixture sources reference the shared asset as `../assets/scan_page_01.png`, so
  a root at `corpus/fixtures/typst` makes that path escape the root and f03 fails to compile. The plan's
  own `World { root: corpus/fixtures, .. }` is right; the virtual path has to sit one level down to match.
- **`ident` is the fixture's file stem, not `Smart::Auto`.** `Auto` hashes the title and author, and
  `f03_image_only.typ` sets `title: none, author: ()` — so it would share a document identifier with any
  other untitled fixture. `timestamp: None` and `today()` pinned to a fixed date keep the clock out of
  the output entirely.
**A finding for tests 0.14–0.16.** Typst 0.15.1 writes `/Creator = "Typst 0.15.1"` and **no `/Producer`
at all**. The plan's expected `inspect --json` has this inverted — it shows `"producer": "Typst 0.15.1"`
and `"creator": null`. The snapshots must follow the file, not the plan. This also makes the `/Creator`
fallback in `producer_family` (item 0.7) load-bearing rather than defensive: without it every Typst
fixture would classify as `Unknown`.
Evidence: `cargo nextest run -p xtask` — `fixtures::typst_fixtures_are_reproducible` passes, so
compiling each source twice yields byte-identical PDFs and R7 §D.2's golden-binary fallback is not
needed. `cargo run -p xtask -- fixtures` produces f01 (14 154 B), f02 (16 390 B), f03 (59 450 B), all
two pages; `--keep-structtree` produces the three `__tagged` variants, all carrying `/StructTreeRoot`.
`pypdfium2` reads the metadata quoted above.
Affects: D18, IMPLEMENTATION_PLAN Phase 0 fixture recipe and the expected `inspect --json`, tests
0.14–0.16, `xtask/src/fixtures.rs`, `corpus/manifest.json`.

## 2026-09-09 · `inspect`: two traits, a leaked library, and geometry at report precision · Phase 0
Context: Phase 0's architecture gives `PdfBackend` with both `open` and `version`, and
`inspect(doc, opts) -> InspectReport`. Building it surfaced three things.
Decision:
1. **`PdfOpen` is a separate trait from `PdfBackend`.** `inspect` needs only "give me a document"; a
   test or a second backend can supply that without also pretending to be a loaded library with a
   version. `PdfBackend` keeps `version()`, which is what the `hello` event carries.
2. **The `Pdfium` handle is leaked (`Box::leak`).** A `PdfDocument` borrows from the `Pdfium` that
   produced it, and `PdfOpen::open` returns an owned `Box<dyn PdfDoc>`, so the library must outlive
   every document it opens. The alternatives are a self-referential struct or a lifetime in a public
   API, and §0.1 forbids lifetimes in public APIs. One process binds one library and then converts, so
   "lives forever" and "lives as long as the process" are the same statement here.
3. **`has_struct_tree` and `encrypted` are byte searches, not parses.** PDFium exposes no predicate for
   either, and `inspect` only needs to know whether one is present. Phase 1 reads the structure tree
   properly through `lopdf` when it needs the hints inside it.
4. **Geometry is rounded to two decimals in the report**, matching canonical JSON's precision
   (ARCHITECTURE §4.3), so a snapshot cannot churn on the last bit of an `f32`.
5. **Image coverage is a sum of bounding boxes, clamped to 1**, not a union. Two overlapping images
   would over-count, but the value is only ever compared against a threshold and the clamp bounds the
   error in the direction that matters.
Confirmed against real files: the fixtures classify exactly as the plan predicts — f01/f02 `text` at
confidence 1.0, f03 both pages `image_only` at 0.9 with `image_count` 1, `image_area_ratio` 1.0 and one
`W_IMAGE_ONLY_PAGES` warning carrying `count: 2`. Page sizes match too: f01 419.53 × 595.28, f02
595.28 × 841.89. The plan's illustrative `visible_chars` (1180/214) are not the real counts (668/256 for
f01, 786/226 for f02); the plan says the snapshot is the assertion, and it is.
The producer/creator inversion recorded in the fixtures entry is confirmed here: `producer` is `null`
and `creator` is `Typst 0.15.1` in all three reports, and `producer_family` is `Typst` only because
`producer_family` falls back to `/Creator`.
Evidence: `cargo nextest run --workspace` — 19 passed. Three committed `insta` snapshots.
Affects: D3, D13.10, IMPLEMENTATION_PLAN Phase 0 architecture and tests 0.14–0.16, `crates/oc-pdf`.

## 2026-09-09 · CLI: hand-rolled parsing, and where the one-based/zero-based line is drawn · Phase 0
Context: §2.1 specifies a large CLI surface; Phase 0 implements one subcommand of it. §1.2's dependency
list gives the binary `anyhow` and no argument parser.
Decision:
1. **Hand-rolled argument parsing, not `clap`.** Phase 0 needs one subcommand and four flags, and §0.2
   asks for the least code that does the job. A parser generator earns its dependency when `convert`
   arrives with twenty flags and a job-spec; that is the phase to add it in, with the `deny.toml` check
   §7 of LICENSE_AND_DEPENDENCIES requires. The parser is one function returning a typed `Command`, so
   swapping it later touches nothing else.
2. **`--pages` is one-based on the command line and zero-based in the IR, converted in exactly one
   place.** A reader counts pages from one and `BlockId` indexes them from zero (D13.3); the conversion
   lives in `parse_page_range` and nowhere else, so the two conventions never meet again.
3. **`--progress json` is detected before parsing can fail.** A usage error must still be reportable as
   a `fatal` event, and it cannot be if the channel is only opened after the arguments parse.
4. **`cmd_inspect::run` returns an exit code, not a `Result`.** An error has to reach the caller both as
   an event on stderr and as an exit code, and §2.4 says never to infer one from the other. Producing
   both in one place is what keeps them from disagreeing.
5. **The report on stdout is pretty-printed.** It is read by a person far more often than by a machine,
   `serde_json`'s pretty printer is deterministic, and test 0.17 checks that two runs are byte-identical.
   The canonical (compact) form belongs to the IR, not to this report.
6. **A binding failure exits 2, not 1.** No PDF was touched, so nothing was attempted; that is the
   definition §2.4 gives for 2 rather than 1.
Evidence: `cargo nextest run --workspace` — 22 passed. Test names land exactly as the plan's table
spells them, because integration tests in `tests/cli.rs` and `tests/events.rs` are reported by nextest
as `openconvert::cli <fn>` and `openconvert::events <fn>`.
Affects: D13.1, D13.2, IMPLEMENTATION_PLAN §2.1–§2.4, tests 0.17–0.19, `crates/openconvert`,
`crates/oc-core/src/{events,exit}.rs`, and Phase 1 (where `convert` will want a parser generator).

## 2026-09-09 · Assertion runner: pending is a third outcome, and the enum is closed · Phase 0
Context: §0.6 fixes the golden-assertion format and its eleven kinds, and the Phase 0 regression note
says the runner "skips assertions whose required stage is not yet implemented and *reports* them as
`pending`, never as `pass`".
Decision:
1. **`Outcome` has three variants, and `Pending` carries the stage that would answer it.** Each
   `Assertion` declares its `required_stage`, and `AssertionDocument`'s fields are all `Option` — `None`
   means that stage has not run. This is what lets a fixture carry its *final* expectations from Phase 0
   while the pipeline is a third built, which is the point of writing them now.
2. **`deny_unknown_fields` and no catch-all variant.** A kind the runner does not know, or a misspelled
   field in one it does, is a parse error. The failure mode being designed against is an assertion that
   silently does nothing, because that reads as passing.
3. **The test asserts its own completeness twice over**: every kind appears exactly once in the sample
   file, and the count is checked against §0.6's eleven. It then evaluates all eleven three times — once
   against a document that satisfies them, once against an empty one (all pending, none a pass), and once
   against one that contradicts them (all failing).
Evidence: `cargo nextest run -p oc-testkit` — 1 test, all eleven kinds through parse, serialise, reparse
and three evaluations each.
Affects: §0.6, test 0.21, `crates/oc-testkit/src/assertions.rs`, every phase that adds a stage.

## 2026-09-09 · `ci-lint` exempts exactly two files, and the exemption is itself asserted · Phase 0
Context: §0.3 item 9 and §0.2 make "no `#[ignore]`" and "no unnumbered TODO" CI gates. A linter that
scans the repository's source cannot describe its own rules without tripping them.
Decision: **the exemption list is exactly `xtask/src/ci_lint.rs` and `xtask/tests/ci.rs`**, and test 0.23
asserts that it is still exactly those two. There is no per-line escape comment — an escape hatch for
"no skipped tests" would be the first thing reached for under deadline. Everything else in the repository
is linted, including the rest of `xtask`; where the usage text and the CI workflow comment quoted the
banned patterns, they were reworded rather than exempted. The test builds its own offending fixtures at
runtime (`format!("#[{}]", "ignore")`), so it does not rely on the exemption it is testing.
Two false positives the first run found and the rules now handle: `models.toml`'s `TODO_COMMIT_SHA`
placeholders are a named-slot convention with their own `--release-branch` rule, not forgotten notes; and
a marker only counts at a word boundary, so `MITODOS` is not a marker.
`thresholds-lint` calls `oc_core::thresholds::lint` rather than reimplementing D17's rule — one
implementation, two callers — and computes today in UTC, printing the date it judged against so nobody
has to guess which clock ran. The civil-from-days conversion is Howard Hinnant's; its constants are
calendar facts, not tunable numbers, so they stay in code (consistent with the format-constants entry).
Evidence: `cargo run -p xtask -- ci-lint` — clean; `thresholds-lint` — clean (2026-09-09). Both tests
also feed the rules text that must fail, so a linter that stopped finding anything cannot pass.
Affects: §0.2, §0.3, §1.9, D17, tests 0.23 and 0.5, `xtask`.

## 2026-09-09 · `unmaintained` is scoped to what a maintainer can act on · Phase 0
Context: adding Tauri (D2) to the shipped tree brings six `unmaintained` advisories, all transitive and
none with an upgrade path: `proc-macro-error` (RUSTSEC-2024-0370) and five `unic-*` crates reached
through `urlpattern` → `tauri-utils`, which is a runtime dependency of `tauri`, not only a build one.
There are **no vulnerabilities** among them. `deny.toml`'s `ignore = []` makes all six hard failures.
Decision: **`unmaintained = "workspace"` in both configs.** Vulnerabilities, unsound code and yanked
releases stay hard failures in the shipped config and always will — those are defects, and a defect that
ships is this project's problem however deep it sits. "Unmaintained" is a different claim: nobody is
patching it. That claim is only *actionable* for a crate this workspace chose, where the response is to
choose differently. Five levels inside Tauri or Typst with no published upgrade, the available responses
are to fork the crate or abandon the framework D2 mandates, and neither is a decision a red CI run should
be forcing on a Tuesday.
This refines what the Q1 entry above says about `deny.toml` keeping "every rule absolute", and it is
worth being plain about that rather than letting the two entries quietly disagree: `exceptions = []` and
`ignore = []` remain, no advisory is listed by id anywhere, and every vulnerability class is still
enforced on the shipped tree. What changed is the *scope* of one advisory class, stated as a rule about
actionability rather than as a list — which is the whole requirement the maintainer set, because a list
needs a new entry every time an upstream tree is reshuffled, and lists like that stop being read.
A useful side effect: the tools advisory report now shows exactly two findings, the two real `quick-xml`
vulnerabilities, instead of burying them under five unmaintained notices.
Evidence: `cargo deny check` — advisories ok, bans ok, licenses ok, sources ok, with Tauri and Typst both
in the workspace (721 crates). `cargo deny --config deny.tools.toml check advisories` — two errors, both
vulnerabilities, reported by the non-blocking CI step.
Affects: D2, SECURITY.md §9, `deny.toml`, `deny.tools.toml`.

## 2026-09-09 · The version handshake lives in the UI, not in the Rust shell · Phase 0
Context: A0.6 wants the window to show the engine version; A0.7 wants it to refuse to start when the
staged sidecar's version differs from the app's (RT A5.7 — the stale-sidecar footgun). Test 0.22 is a
Vitest test with a Tauri mock.
Decision: **the handshake is TypeScript, behind a `Spawner` interface.** The real implementation wraps
Tauri's sidecar `Command`; the test passes a stub. Putting it in the Rust shell would have made A0.7
testable only by launching a window, which is exactly the kind of test D7 rules out — and the shell then
has no logic to get wrong. `handshake` checks three things in order, each with its own error kind:
protocol, `ir_version`, then engine version against the app's.
`parseEvents` stops at the first line that does not parse rather than skipping it. Something writing
non-JSON to the engine's stderr — a linker warning, a sanitizer, a crash handler — is usually the
interesting part of the failure, and silently dropping it loses the only evidence there is.
`xtask stage-sidecars` copies the engine under the `<name>-<triple>` name Tauri's `externalBin` expects
and writes a `STAGE_STAMP` beside it, recording the version the staged binary itself reports rather than
one read from a manifest — so the stamp describes what is actually on disk.
The Tauri capability allows exactly one program, the sidecar, and the CSP sets `connect-src 'none'`
(D13.9): the webview has no network permission at all.
Evidence: `npm test` in `apps/desktop/ui` — 5 tests, covering the hello parse, the A0.7 refusal, the
protocol and IR mismatches, a stream that does not begin with `hello`, and the non-JSON line.
`cargo run -p xtask -- stage-sidecars` staged `openconvert 0.1.0` for the host triple.
Affects: D2, D7, D13.9, RT A5.7, RT B15, A0.6, A0.7, test 0.22, `apps/desktop`, `xtask`.

## 2026-09-09 · Verification-debt stubs VD-b…VD-g · Phase 0
Acceptance A0.9 requires every row of the Phase 0 verification-debt table to carry an owner, a blocking
phase, and a stub here. VD-a is closed above. The rest are open, and each closes with a dated entry in
this file naming the source actually consulted. An open row past its blocking phase is a release blocker,
not a warning. **Owner: maintainer, for every row.**

- **VD-b — `hyphenation` pattern licences. Blocks Phase 3.** The crate is Apache-2.0/MIT, already
  confirmed; the bundled TeX/`hyph-utf8` *pattern files* are separately licensed and unverified, and it is
  also unconfirmed that DE, TR and EN patterns are present at all. To close: read the pattern files'
  own licence headers in the version pinned at that time, and record which of the three languages ship.
- **VD-c — `zspell` licence. Blocks the optional dictionary pack (post-v1).** crates.io reports the
  licence field as literally "Non-standard", unmapped to any SPDX id. `zspell` is not a dependency today
  and `deny.toml` would reject it, so nothing in v1 waits on this. To close: read the crate's own
  `LICENSE` file.
- **VD-d — PDFium SMask, vector-path and bookmark coverage. Blocks Phase 1, and Phase 4's image policy.**
  An API-signature-level spike over ten real PDFs (SMask, stencil mask, CMYK JPEG, indexed PNG, 1-bit
  CCITT, JPX, inline image, rotated image, tiny ornament, full-page scan), comparing
  `get_processed_image()` against a `pypdfium2` reference. This is Phase 1's image spike; the row exists
  so it is not quietly dropped, because D3's own residual-risk note depends on the answer.
- **VD-e — igerman98 and Turkish hunspell licences. Blocks the optional dictionary pack (post-v1).**
  D15 routes around this for core data by generating the word-frequency lists from CC0/PD text, so
  nothing in v1's shipped path depends on the answer. To close: read both projects' licence files.
- **VD-f — validation-pack JRE licence, per vendor. Blocks Phase 6, or whenever the validation pack
  ships.** That a Temurin (or other OpenJDK-derived) minimal `jlink` image is redistributable under
  GPLv2 + Classpath Exception has to be read from that vendor's own licence text, not assumed from
  OpenJDK generally.
- **VD-g — UB-Mannheim Windows Tesseract. Blocks Phase 13.** The installer's actual install path and the
  Tesseract version it delivers, so the Windows discovery probe looks in the right place for the right
  binary.

## 2026-09-09 · PDFium already performs overdraw dedup, and does not lose distinct glyphs · Phase 1
Context: D13.4 lists `OverdrawDedup` as a ledger reason with a 0.02 budget, and Phase 1 detail 2 defines
the rule: "two glyphs with identical `ch`, `font`, `size_pt` whose origins differ by < 0.35 pt in both
axes are the same glyph drawn twice (fake bold, R1 §D.6 #2) → keep one". Test 1.4 asserts `C_raw` holds
two of the character and `C_0` one.
Measured (PDFium 151.0.7881.0, through `FPDFText_*` on hand-built one-page PDFs, 12 pt Helvetica):
- Two **identical** glyphs at 0.0, 0.1, 0.2, 0.3, 0.34, 0.5, 1, 2, 3 and 4 pt apart → `count_chars` is
  **1**. At 5 pt and beyond → 2. So PDFium collapses an overdrawn duplicate itself, at a separation an
  order of magnitude larger than the plan's 0.35 pt.
- Two **different** glyphs (`A`/`B`) at 0.2, 1 and 2 pt apart → **2** characters, both origins reported.
  A combining accent over a base letter likewise stays two characters. **PDFium does not merge distinct
  characters**, so nothing is lost.
Consequences, all of which change Phase 1 rather than any decision:
1. **`C_raw` taken from the text page is already post-overdraw-dedup.** The rule in detail 2 would never
   fire: by the time we see glyphs, the duplicate is gone.
2. **Test 1.4 as written cannot hold through this API** — `C_raw` will have one of the character, not
   two. It has to assert the observable truth instead: the fixture's two draws arrive as one glyph, and
   nothing is silently dropped from a *different* pair.
3. **The 0.02 budget loses its subject unless the removal is measured.** The intent of D13.4 is that
   dedup cannot quietly eat text, and that intent survives only if the amount PDFium removed is
   observable. The way to keep it is to count glyphs a second way — from the page's text *objects*, whose
   strings are pre-dedup — and ledger the difference as `OverdrawDedup`. That keeps the reason, the
   budget and the invariant, with PDFium doing the detection and us doing the accounting.
No decision in DECISIONS.md is contradicted: D13.4 says dedup happens and is budgeted, not who performs
it. Recorded here rather than blocked because the guarantee is preserved by (3), and because the
measurement — distinct characters are never merged — is the part that could have been alarming and is not.
Re-checkable on a PDFium bump: `oc_testkit::handmade::{overdraw_at, overlap_pair_at}` build the fixtures
the probe used.
Affects: D13.4, IMPLEMENTATION_PLAN Phase 1 detail 2 and test 1.4, VD-d (the wider PDFium spike).
