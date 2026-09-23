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

## 2026-09-09 · Glyph extraction: four things the tests found · Phase 1
Context: item 1.2 implements `oc-model::{extract,ledger}` and `oc-pdf::glyphs` against tests 1.1-1.4.
Four findings, each one a test failing for a reason worth keeping:

1. **`PdfiumBackend::bind()` is now idempotent.** PDFium initialises global state, so a second
   `bind_to_library` in one process fails with `PdfiumLibraryBindingsAlreadyInitialized`. Test 1.4 opens
   two fixtures and hit it immediately. A caller should not have to know this: the first successful bind
   is cached in a `OnceLock` and every later call returns it. A *failed* bind is not cached, so fixing
   `OC_PDFIUM_PATH` and retrying still works. Consequence for test 0.7: once a process has bound, `bind`
   no longer consults the environment, so the "the override is authoritative" assertion moved to a new
   `PdfiumBackend::resolve_library()`, which is the function that actually implements the rule. Test 0.7
   now also asserts that binding twice succeeds.

2. **`PageGeometry::normalise`'s invariant is conditional, and had to be.** Phase 0 detail 5 asks for a
   `debug_assert!` that every produced rect lies inside the page. That is only true of rects that started
   inside the crop box; a glyph outside it is content the page clips away, which is exactly what the
   `ClippedOffPage` ledger reason exists for. As written the assertion made a *correct* extraction panic.
   It now reads "a rect inside the crop box must land inside the page", which is the invariant that was
   meant and the one R1 §D.6 #1 is about.

3. **The overdraw measurement is not available through this backend.** The previous entry proposed
   keeping D13.4's `OverdrawDedup` budget meaningful by counting characters a second way, from the page's
   text objects, and ledgering the difference. Measured: `PdfPageTextObject::text()` returns the *same*
   deduplicated text - one character for a fixture whose content stream draws two - because it reads
   through the text page. So the count PDFium collapsed cannot be recovered from `pdfium-render`'s API at
   all. Rather than ship a field that is always zero, `objects_char_count` and `overdrawn_by_backend` are
   gone and test 1.4 asserts what is true and checkable: the duplicate does not survive, we did not
   remove it, and two *different* overlapping glyphs at the same separation both do survive. **Open, for
   the conservation-law work in Phase 2/6:** recovering the collapsed count needs content-stream access
   through `lopdf` (counting the bytes shown by `Tj`/`TJ`), which is the only route left. Until then
   `OverdrawDedup` has a budget and no way to consume it, which is safe - it cannot hide a loss, because
   PDFium never merges distinct characters - but it is not the guarantee D13.4 describes.

4. **PDFium reports font weight 0 for the standard fourteen**, which carry no `FontDescriptor`. Zero
   would read as "thinner than hairline" to the heading clusterer in Phase 4, so a missing or zero weight
   becomes 400, which is what a reader renders those fonts at.

Also corrected: h05 was not an OCR sandwich. D13.10 recognises one from three signals together -
invisible text, a glyph-less font, and an image covering the page - and the fixture had only the first,
so it classified as `blank` and test 1.3 would have asserted against a rule that never fired. It now
carries a full-page image and declares its font `GlyphLessFont`. Its alphabet also ran off the right edge
of a 200 pt page, which would have made it a clipping test by accident.
Evidence: `cargo nextest run --workspace` - 30 passed.
Affects: D3, D13.4, D13.10, R1 §D.6 #1, IMPLEMENTATION_PLAN Phase 0 detail 5 and Phase 1 details 1-3,
tests 0.7 and 1.1-1.4, `crates/oc-model/src/{extract,ledger}.rs`, `crates/oc-pdf`.

## 2026-09-09 · PDFium reorders text under `/Rotate 90`, and two mutation decisions · Phase 1
Context: item 1.3 writes the metamorphic invariants, tests 1.5-1.7, in
`crates/oc-pdf/tests/metamorphic.rs`.

1. **PDFium's character order is not rotation-invariant, and test 1.5 as the plan states it is
   false.** The plan's assertion is "for `/Rotate` in {0,90,180,270} applied to `f01`, the ordered
   sequence of `ch` values is identical". Measured, over both pages of `f01`:

   | `/Rotate` | raw order preserved | multiset preserved | order preserved after un-rotating |
   |---|---|---|---|
   | 0   | yes | yes | yes |
   | 90  | **no** | yes | yes |
   | 180 | yes | yes | yes |
   | 270 | yes | yes | yes |

   At 90 - and only at 90 - PDFium's text page hands back the lines of the first paragraph in a
   different order. Its text page is not a transcript of the content stream: it groups characters
   into text lines and picks a flow orientation, and at a quarter turn that heuristic decides
   differently. Nothing is lost, and no line is internally scrambled; the lines arrive in another
   sequence.

   So the invariant is asserted where it is true rather than weakened to a multiset comparison.
   The test un-rotates each glyph origin back into the unrotated display frame and compares the
   characters **in reading order**, which is the same string for all four rotations. That is a
   stronger test than the plan's, not a weaker one: it also proves `PageGeometry::normalise` maps
   each rotation to the right axis, because origins from a rotation we had mapped wrongly would
   not line up. The multiset is still asserted, first and separately, so that a failure says
   whether text was *lost* or merely *reordered* - which is the difference between a conservation
   bug and an ordering bug.

   Consequence for later phases: **no stage may treat the backend's glyph order as reading order.**
   Phase 3 computes reading order from geometry, which was always the design (PIPELINE `layout`),
   and this is the measurement that says the shortcut was never available.

2. **The mutation recipes are Rust, in `oc-testkit::mutate`, not Python.** The plan's Phase 1 file
   list puts them in `eval/src/oc_eval/mutate/*.py`. A metamorphic test has to mutate and compare
   in one process; a Python step in the middle would make `cargo nextest` - the gate - depend on an
   interpreter, a virtualenv and a `pikepdf` wheel to run at all. `rotate` and `cropbox_offset` are
   implemented over `lopdf`, which is the first real use of that dependency. The Python recipes keep
   their job in Phase 7, where a corpus of real books is mutated once and the output is a file.

3. **Mutated fixtures are committed, by a new `cargo xtask mutations`.** `corpus/fixtures/mutations/`
   was an empty directory the plan names as a regression artefact. It now holds
   `h01__cropbox_offset.pdf`, and test 1.6 ingests all three routes to the same page - `h03`, built
   with the offset; `h01` mutated at run time; and the committed file - because they fail
   differently. The committed file is compared by what it extracts, not byte-for-byte, so an `lopdf`
   bump that writes the same document differently does not fail an unrelated test.

Also: `Page::line_of_glyphs` in `oc-testkit::handmade` builds one line of eight glyphs in any given
operator order, for test 1.7. The gap between them is deliberately wide enough that PDFium
synthesises spaces, which ingestion then drops - a reorder test that counted those would be testing
the wrong invariant.
Evidence: `cargo nextest run --workspace` - 33 passed. Rotation table measured on PDFium
`chromium/7881` with `f01_prose_single_column.pdf`.
Affects: D3, D13.4, R1 §D.6 #1, IMPLEMENTATION_PLAN Phase 1 tests 1.5-1.7 and its Files list,
`crates/oc-pdf/tests/metamorphic.rs`, `crates/oc-testkit/src/{mutate.rs,handmade.rs}`,
`xtask/src/mutations.rs`.

## 2026-09-09 · The broken-text detector missed the commonest broken text · Phase 1
Context: item 1.4 writes test 1.8, `stripped_tounicode_page_classifies_broken_text`, over a new
`strip_tounicode` mutation of `f01`.

**The finding.** Stripping `/ToUnicode` from `f01` and inspecting it gave, on both pages:
`class: Text, class_confidence: 1.0, visible_chars: 664, replacement_chars: 0, pua_chars: 0`.
The extracted text is
`"\u{1}\u{2}\u{3}\u{4}\u{1}\u{3}\u{5}\u{6}\u{4}..."` where the original reads
`"The Test BookChapter 3It was a dark and stormy night..."`. Typst writes Type0/CIDFontType0 fonts
with Identity-H encoding, so without a `/ToUnicode` map PDFium hands back **the glyph indices
themselves** - U+0001, U+0002, U+0003 - which are neither U+FFFD nor private-use.

So the Phase 0 detector, which counts `replacement + pua` over `visible`, scored a page of pure
mojibake as ordinary text at full confidence. That is the exact failure D13.10 and PIPELINE §658
exist to prevent: the page would have been emitted into the EPUB as 664 control characters instead
of being routed to OCR. `f01` unmutated classifies `text` at confidence 1.0 too, so the test was
worth having precisely because it could move the classifier rather than agree with it.

**The fix, and why it is a completion rather than a new decision.** PIPELINE line 154 states the rule
as "U+FFFD/PUA share above the language prior **or** dictionary hit rate below it", and line 158
records that the whole share statistic is already a *substitution* for the missing
`has_unicode_map_error()` (V2 §1, RT B1). The intent - "characters that decode to nothing usable" -
is unchanged; the enumeration of what that looks like was incomplete. `PageCharStats` gains a third
counter, `control`, and `is_broken_text` sums all three over the same threshold. Nothing about the
threshold, the ordering of the arms, or the second (dictionary) arm changes. `pageclass.broken_text_
replacement_share` keeps its value of 0.20; only its `evidence` string is restated.

**PDFium marks a line-break hyphen with U+0002 and `is_hyphen()`.** Measured while checking the new
counter for false positives: `f02` page 0 carries exactly two control characters, at `projec-tion`
and `reading-order` - both hyphenated across a line break in the two-column layout - and both have
`is_hyphen()` set. The stripped `f01` carries 653 controls and **not one** is flagged. The flag
separates the two meanings exactly, so `is_undecodable_control` excludes hyphen-flagged characters
and a heavily hyphenated page is not charged for its own hyphens. Tab, line feed and carriage return
are excluded too: PDFium inserts those between lines and columns as structure.

**Open, for Phase 2/3.** Two things follow from that hyphen marker and neither is Phase 1's:
- `C_raw` for a hyphenated page contains U+0002 where the document contains U+002D. The
  conservation quantity is therefore already one character away from the document at every line-break
  hyphen. Normalisation `N` has to map it back, before `Dehyphenate` can account for removing it.
- It is a free, exact dehyphenation signal - PDFium is naming the line-break hyphens - which the
  Phase 3 dehyphenator should use rather than re-deriving from geometry.
Evidence: `cargo nextest run --workspace` - 34 passed. Measured on PDFium `chromium/7881`.
Affects: D13.10, PIPELINE lines 154/158/658, R2 §B.8, RT B1, `thresholds.toml`
`pageclass.broken_text_replacement_share`, `crates/oc-pdf/src/{classify.rs,inspect.rs,pdfium/doc.rs}`,
`crates/oc-testkit/src/mutate.rs`, the three `inspect` snapshots (new `control_chars` field).

## 2026-09-09 · Images: two backends, and two flags that would otherwise never be true · Phase 1
Context: item 1.5 implements `oc-pdf::images` and `PdfDoc::page_images` against test 1.9.

1. **The plan's DPI band assumes a page size the fixture does not have.** Test 1.9 asserts
   `effective_dpi` within [140, 160]. `scan_page_01.png` is 1240 x 1754, which is A4 at 150 dpi -
   so the band is right for an A4 page. But `f03_image_only.typ`, also given verbatim by the plan,
   sets `page(width: 148mm, height: 210mm)`: A5. The same asset over 419.53 pt is
   1240 / (419.53 / 72) = **212.81 dpi**. The two halves of the plan contradict each other.
   Resolved in favour of the fixture: 212 dpi is an ordinary scan resolution, `f01` and `f02` are
   A5 too so changing only `f03` would make the corpus inconsistent, and the test's purpose is that
   the division is right rather than that it equals 150. `intrinsic_px` is asserted alongside the
   result, so both inputs and the output are pinned and a right answer cannot come from wrong parts.

2. **`has_smask` and `is_inline` cannot come from PDFium, so they come from the file.** PDFium's
   image object exposes width, height, colour space and DPI, and nothing about masks or about
   inline images; `FPDFImageObj_GetImageMetadata` has no field for either. Both are properties of
   the file, so `pdfium::images` walks the page content stream with `lopdf`: `/Name Do` resolved
   through `/Resources /XObject` gives the mask flag from `/SMask` or `/Mask`, and `BI` gives an
   inline image directly. `PdfiumDoc` now keeps the `lopdf::Document` beside the PDFium one.

   The two sides are matched **by draw order** - PDFium enumerates page objects in content-stream
   order and so does the walk - and the match is used **only when both agree on the count**.
   Otherwise both flags read `false` for every image on the page. That is the safe direction: an
   unnoticed mask makes compositing do work it need not; a wrongly asserted mask would drop pixels.

3. **Two fixtures added, because otherwise both flags could only ever be false.** `f03` has no mask
   and no inline image, so test 1.9's `!has_smask` and `!is_inline` would pass just as well against
   a detector hard-wired to `false` - which is exactly what the fallback in (2) is. `h09_image_smask`
   is a full-page image with an 8-bit `/SMask`, and `h10_inline_image` writes `BI ... ID ... EI`
   straight into the content stream (`pdf-writer` has no inline-image API; the content stream is
   bytes). Each is asserted true, and each test also asserts the negative case through the same
   code path, so the two answers are told apart rather than one being the only one available.
   `h09` is also the seed of the VD-d spike, which needs a masked image to compare against.

4. **Three thresholds added** under `[images]`: `full_page_area_ratio` 0.95, `strip_aspect_ratio`
   8.0, `ornament_max_side_pt` 48.0, all from Phase 1 detail 4 and all `provisional`. The
   full-page test runs before the strip test, which matters on a tall narrow page: a page-shaped
   background is not a banner. `image_kind_covers_every_arm` pins all four arms plus that ordering,
   because three of the four are otherwise reachable only through fixtures that do not exist.
Evidence: `cargo nextest run --workspace` - 38 passed. Measured on PDFium `chromium/7881`.
Affects: D13.11, IMPLEMENTATION_PLAN Phase 1 detail 4 and test 1.9, IR_SKETCH `ImageRef`,
`thresholds.toml` `[images]`, `crates/oc-model/src/extract.rs`,
`crates/oc-pdf/src/{images.rs,inspect.rs,pdfium/{doc.rs,images.rs}}`,
`crates/oc-testkit/src/handmade.rs`.

## 2026-09-09 · Resource limits: where they are checked, and two substituted magnitudes · Phase 1
Context: item 1.6 implements `oc_core::limits` and `oc_pdf::limits` against tests 1.10, 1.11, 1.20.

1. **`max_image_pixels` is a pixel count, not a byte count.** The plan's detail 8 writes the check
   as `width * height * bpc / 8` compared against `limits.max_image_pixels`, which compares bytes
   to pixels. The threshold's own evidence line names Pillow's `MAX_IMAGE_PIXELS`, which is a pixel
   count, and the value 100 000 000 is that order of magnitude. Implemented as pixels. Bits per
   component vary by at most an order of magnitude and the allowance has two to spare, so the unit
   choice does not change which files are refused - but it changes what the error message means,
   and a limit whose number means something different from its name is a limit nobody can tune.

2. **`open_with_limits` is the door.** `PdfOpen` gains a limits-carrying open and `open` defaults to
   the shipped set. The page-count guard runs inside it, immediately after PDFium reports the count
   and before any page is touched. That is what test 1.20 means by "at the door": a guard on first
   page access would let a hundred-thousand-page document cost a hundred thousand page parses
   before declining. `PdfiumDoc` holds its `Limits` for the life of the document - a limit that can
   change halfway through is not a limit - and `InspectOptions` carries them so `--max-pages`
   reaches it.

3. **A limit refusal is exit 2, not exit 1**, with its own fatal code `E_LIMIT_EXCEEDED`. §2.4 gives
   exit 2 to "usage, job-spec or configuration error; nothing was attempted", and that is what this
   is: the file is outside the budget this run was configured with, and the operator's next move -
   raise the limit, or reject the file - is the same whether the refusal came at the door or three
   hundred pages in. One code for both, rather than a distinction that changes nothing.

4. **Two magnitudes in test 1.11 are substituted, deliberately.** The plan asks for a stream
   declaring 8 GiB and an assertion that process RSS stays under 300 MB. The fixture expands to
   8 MiB from 8.9 KB - a ratio near a thousand to one, which is the attack - and the refusal is
   demonstrated against a 1 MiB cap rather than the shipped 256 MiB one. Reasons: proving the
   inequality at the shipped cap would mean allocating a quarter of a gigabyte in CI to watch a
   comparison succeed, and asserting a process RSS figure measures the allocator rather than the
   code. Peak memory is bounded by construction - `lopdf` caps each filter layer as it decodes, so
   nested filters cannot expand past the cap once per layer either - and the test asserts the
   second half that stops it passing vacuously: under the shipped cap the same fixture reads fine,
   so the refusal is the limit acting rather than the file being unreadable.

5. **The cap is applied where the content is actually read.** `read_page_content` was, for one
   commit, called only from its own test while `page_image_facts` still used the uncapped
   `get_and_decode_page_content` - a limit that exists and enforces nothing. `page_image_facts` now
   returns `Result<Option<_>>`: `Ok(None)` still degrades when `lopdf` simply cannot parse a page,
   but a *limit* refusal propagates, because reading a hostile stream and then shrugging is worse
   than not reading it. Test 1.11 asserts both routes.

Fixtures added: `h11_pixel_bomb` (an image XObject declaring 40 000 x 40 000 behind a 64-byte
stream) and `h12_decompression_bomb` (8.9 KB of deflate over 8 MiB of content-stream whitespace,
which keeps the file a valid PDF that renders normally). `oc_testkit::handmade::many_pages(n)` is
*not* committed as a fixture: three thousand empty page dictionaries are a third of a megabyte that
nobody would review, and the only thing the test needs from them is that there are 3001 of them.

`check_xref_chain` exists and has no caller yet. `lopdf` does its own xref traversal and exposes no
hook; the check is there for the parse path Phase 14 owns, and it is unit-tested rather than
pretended to be wired.
Evidence: `cargo nextest run --workspace` - 45 passed.
Affects: D13.2, R8 §A2, IMPLEMENTATION_PLAN Phase 1 detail 8 and §2.1/§2.4, `thresholds.toml`
`[limits]`, `crates/oc-core/src/limits.rs`, `crates/oc-pdf/src/{limits.rs,error.rs,inspect.rs}`,
`crates/oc-pdf/src/pdfium/{bind.rs,doc.rs,images.rs}`, `crates/openconvert/src/{cli.rs,cmd_inspect.rs}`,
`crates/oc-testkit/src/handmade.rs`.

## 2026-09-09 · Encryption: fixtures built rather than committed blind, and one error split · Phase 1
Context: item 1.7 implements `oc-pdf::encrypt` against tests 1.12, 1.13, 1.14.

1. **Encrypted fixtures are generated, not obtained.** `pdf-writer` cannot encrypt, so the obvious
   route was to produce three encrypted PDFs out of band once and commit them - which would make
   them the only fixtures in the corpus nobody could regenerate or inspect the provenance of.
   `lopdf` 0.45 turns out to expose the whole standard security handler (`EncryptionVersion::V4`,
   `Aes128CryptFilter`, `Permissions`, `Document::encrypt`), so `oc_testkit::mutate::encrypt` is a
   recipe like the other three and `cargo xtask mutations` writes `h01__encrypted_empty_user.pdf`,
   `h01__encrypted_password.pdf` and `h01__encrypted_no_print.pdf`. AES-128 rather than 40-bit RC4
   or AES-256 because it is what the great majority of encrypted PDFs in circulation use, and what
   test 1.12 names. PDFium reads all three, so the two libraries agree about the format.

   The fixture passwords - `owner` and `secret` - are public constants in `xtask::mutations`. A
   fixture password is a test input, not a secret, and a test that cannot say which password it
   used is a test nobody can reproduce.

2. **`PdfError::Open` is split, because 1.13's contract is the distinction.** PDFium reports "wrong
   password" and "this is not a PDF" through one error type, and `open` was collapsing both into
   `PdfError::Open { message }`. A supervising UI that has to parse that message to decide whether
   to prompt for a password cannot be written against a stable contract, so the password case is
   now `PdfError::PasswordRequired`, recognised from
   `PdfiumInternalError::PasswordError`, and the CLI maps it to **exit 2** with
   `fatal{E_PASSWORD_REQUIRED}` - the same "nothing was attempted, your move" code that
   `E_LIMIT_EXCEEDED` established in item 1.6.

3. **An unreadable permission flag reads as *permitted*.** This is the opposite of the safe
   direction everywhere else in the codebase, and deliberately so. Elsewhere an unknown answer
   should restrict; here an unknown answer that restricted would invent a limitation on the user's
   own book out of a flag PDFium merely failed to report. D13.11 says permissions are recorded and
   never enforced, so the field is documentation, and documentation that guesses "forbidden" is
   worse than documentation that guesses "allowed".

4. **`print` is a disjunction.** `pdfium-render` offers `can_print_high_quality()` and
   `can_print_only_low_quality()` and no plain `can_print()`. A file that permits low-quality
   printing permits printing, so `permissions.print` is the OR of the two and
   `permissions.print_high_quality` carries the finer answer. Test 1.14 asserts both are false on
   `h01__encrypted_no_print`, and that the *same* document encrypted with printing allowed reports
   the opposite - without which the assertion would pass against a hard-wired `false`.

The three `inspect` snapshots change by the added `document.permissions` object only.
Evidence: `cargo nextest run --workspace` - 50 passed.
Affects: D13.11, RT D12, IMPLEMENTATION_PLAN Phase 1 detail 7 and §2.4,
`crates/oc-pdf/src/{encrypt.rs,error.rs,inspect.rs,pdfium/doc.rs}`,
`crates/oc-testkit/src/mutate.rs`, `crates/openconvert/src/cmd_inspect.rs`, `xtask/src/mutations.rs`.

## 2026-09-10 · Outlines by walking, and two byte searches replaced · Phase 1
Context: item 1.8 implements `oc-pdf::{outline, meta}` against test 1.15 and Phase 1 detail 6.

1. **The outline is walked, not iterated.** `PdfBookmarks::iter()` flattens the tree and hands
   back a sequence with no depth, and the depth is half the information an outline carries - a
   table of contents whose levels are lost is a list. So `read_outline` walks `first_child` and
   `next_sibling` with an explicit stack, pushing siblings before children so that popping yields
   prefix order. Bounded by a new `limits.max_outline_entries` (100 000): `/First` and `/Next` are
   a linked structure in a file anyone can write, and a cycle has no other stop.

2. **`f01` cannot test what test 1.15 is about.** It has an outline - Typst writes one - but it is
   a single entry, "Chapter 3". One entry cannot tell a depth-first walk from a breadth-first one,
   and it cannot tell a walk that comes back up from one that stops at the first leaf. `h13_outline`
   carries six entries over three levels with two roots, which distinguishes all three, and its
   expected order is a public constant `OUTLINE_TREE` that the builder and the test both read - so
   the assertion cannot drift into a copy of whatever the builder happened to emit. `f01` is
   asserted too, as the case that proves the reader works on a file we did not write.

3. **`lopdf` decrypts on load and removes `/Encrypt` from the trailer.** The obvious predicate,
   `document.trailer.has(b"Encrypt")`, is false for every encrypted document that `lopdf` could
   open - because opening it is what removed the entry. The evidence that survives is
   `Document::encryption_state`, so `is_encrypted` checks that first and the trailer second (for a
   document loaded some other way). Caught by the test, which is the only reason it is not a
   silent "no document is ever encrypted".

4. **Two byte searches replaced by object lookups.** `read_metadata` decided `encrypted` and
   `has_struct_tree` by searching the raw file for `/Encrypt` and `/StructTreeRoot`. Both find the
   string wherever it occurs - inside a content stream, inside a text string - so a document
   *about* PDF accessibility reported a structure tree it does not have, which is precisely the
   corpus this project gets pointed at. Both now come from the object tree, with the byte search
   kept as the fallback for a file `lopdf` cannot parse but PDFium opened: a wrong "yes" on a
   structure-tree hint costs a look (D3 makes it a hint only), and a wrong "no" on encryption would
   be a lie in the report.

5. **XMP is kept as bytes and read for three fields.** `dc:title`, `dc:creator` and `dc:language`,
   scanned with `quick-xml` rather than modelled as RDF: a general RDF parser is a dependency and
   an attack surface in exchange for three strings, and the packet is kept whole anyway so nothing
   is lost by the narrow read. A packet that is not UTF-8 yields nothing rather than a lossy
   decode - a plausible wrong author name is worse than no author name. Which of XMP and `/Info`
   wins when they disagree is Phase 4's question, and Phase 1 only has to make both available.

Note for Phase 4: `read_xmp` is written and tested but nothing calls it yet - `DocMetadata` still
carries only the `/Info` values. Wiring the precedence rule is metadata work and belongs with the
rest of it.
Evidence: `cargo nextest run --workspace` - 57 passed.
Affects: D3, D13.10, D18, IMPLEMENTATION_PLAN Phase 1 detail 6 and test 1.15, IR_SKETCH
`OutlineEntry`, `thresholds.toml` `limits.max_outline_entries`,
`crates/oc-model/src/extract.rs`, `crates/oc-pdf/src/{outline.rs,meta.rs,inspect.rs,pdfium/doc.rs}`,
`crates/oc-testkit/src/handmade.rs`.

## 2026-09-10 · Truncation barely fuzzes anything; corruption does · Phase 1
Context: item 1.9 writes test 1.16 as `crates/oc-pdf/tests/fuzz_lite.rs`.

**The measurement.** The plan's generator is "20 000 random byte strings (and 200 truncations of
real fixtures)". Measured, over a hundred evenly spaced cuts of each of three committed fixtures:

| fixture | bytes | cuts that opened | cuts that extracted glyphs |
|---|---|---|---|
| `h01_two_glyphs` | 717 | 3 / 101 | 3 |
| `h13_outline` | 1535 | 2 / 101 | 2 |
| `h09_image_smask` | 1255 | 2 / 101 | 2 |

Two to three per cent. PDFium wants a trailer and an xref at the *end* of the file, so a cut
anywhere before them is refused at the door - and a cut after them is not a truncation. Two hundred
truncations therefore buy roughly five inputs that reach an extraction path, which is not a fuzz
test of the extraction paths. The same three fixtures with **one byte changed**:

| fixture | opened | extracted glyphs |
|---|---|---|
| `h01_two_glyphs` | 187 / 200 | 170 |
| `h13_outline` | 195 / 200 | 187 |
| `h09_image_smask` | 192 / 200 | 181 |

93-98 % open, 85-94 % extract. Flipping a byte leaves the trailer intact, so PDFium takes its
damaged-file recovery path - rebuilding the xref, guessing object boundaries - which is the
deepest and least-travelled code in the library, and the shape a real damaged file actually has.

**The decision.** Both generators are kept. The plan names truncation and it costs almost nothing;
corruption is added at 2 000 cases because it is the one that fuzzes the code under test. Three
proptest tests rather than one, so the case counts can differ by two orders of magnitude and a
failure names which kind of input found it.

The random half is also improved: half of the random inputs are given a real `%PDF-1.7` header,
because bytes without one are rejected before a parser sees them and twenty thousand of those
measure the header check twenty thousand times.

**The measurement is asserted, not just written down.** `corruption_reaches_the_extraction_paths`
re-runs a deterministic 200-sample sweep and fails if fewer than half reach glyph extraction. A
reach figure recorded only in a comment is one that stops being true silently; if PDFium's recovery
path ever tightens, this says so rather than leaving 2 000 cases quietly testing the door.

**What none of this can catch.** A segmentation fault inside PDFium is not a panic and
`catch_unwind` will never see it - it takes the process with it. RT A5.2 accepts that for v1 and
reserves `--isolate-parser` for the Phase 14 worker. A green run here means "no panic", not "no
crash". Nothing crashed across ~22 200 inputs on `chromium/7881`.
Evidence: `cargo nextest run --workspace` - 61 passed. The fuzz binary is ~12 s, now the slowest
test in the suite by an order of magnitude, which is the price of the plan's 20 000 figure.
Affects: RT A5.2, IMPLEMENTATION_PLAN Phase 1 test 1.16, `crates/oc-pdf/tests/fuzz_lite.rs`.

## 2026-09-10 · The dump is NDJSON, and CharHistogram stops leaking its representation · Phase 1
Context: item 1.10 implements `oc-pdf::dump` and `openconvert dump-stage` against test 1.17.

1. **The dump is one canonical-JSON object per line, not one document.** RT B4 puts a real book's
   extraction layer at tens of megabytes and Phase 1 detail 9 says to stream it per page. A single
   top-level array cannot be streamed without either holding the whole document or hand-rolling the
   commas, and it cannot be read with `head` or grepped by page. So: a header line, then one line
   per page. Peak memory is one page regardless of the book, and a nine-hundred-page dump starts
   appearing immediately rather than after a minute of silence.

2. **`CharHistogram` had a `derive(Serialize)` that leaked its representation.** It stores ASCII in
   a flat `Vec<u32>` of 128 slots because that is where nearly every character in a Latin-script
   book lands - a good decision for the counting, a terrible one for the JSON. Derived, every page
   of every dump carried 128 mostly-zero entries: unreadable in a diff, and megabytes of nothing
   over a book. It now serialises as what it *is* - a map from character to count, in code-point
   order, which is exactly what `iter` already yields. Its own test asserts that, separately from
   the snapshot, because 128 zeroes look like noise and noise is what gets skimmed past.

3. **`dump-stage` is the second subcommand, and the parser grew a branch rather than a dependency.**
   §2.1 says the CLI is hand-rolled until `convert` arrives with its twenty flags. Two subcommands
   with four shared flags is still under that line. The stage name is positional and first, because
   "dump a stage" without saying which is not a request; a stage that has no dump yet is
   `E_UNKNOWN_STAGE` and **exit 2**, since it is a usage error rather than a failed conversion.

4. **The snapshot is of the canonical text, not of a serde value.** `insta::assert_snapshot!` over
   the string `to_canonical_json` produced, rather than `assert_json_snapshot!` over the structure.
   That makes the snapshot a test of the canonical form itself - key order, two-decimal geometry,
   `ir_version` first - which is otherwise only tested by Phase 0's own unit tests on synthetic
   input. A change to D13.3's rendering now shows up here, on real extracted data.

Note: `h01` classifies as `blank`, not `text`, in the snapshot. Two visible characters is below
`pageclass.text_min_visible_chars`, and that is the classifier working - a two-glyph test fixture is
not a page of prose. Worth knowing before someone reads the snapshot and files a bug.
Evidence: `cargo nextest run --workspace` - 65 passed.
Affects: D13.3, D13.8, RT B4, IMPLEMENTATION_PLAN Phase 1 detail 9 and test 1.17, §2.1, §2.4,
`crates/oc-model/src/extract.rs`, `crates/oc-pdf/src/dump.rs`,
`crates/openconvert/src/{cli.rs,cmd_dump_stage.rs,main.rs}`.

## 2026-09-10 · PDFium erases the difference between a soft hyphen and a hard one · Phase 1
Context: item 1.11 writes test 1.18, the differential test against `pdftotext`.

**The finding, and it is the most consequential of Phase 1.** The oracle test over `f02` failed
with exactly one missing word: `"projec\u{ad}"`. Chased down:

| fixture | what the source says | `pdftotext` reports | PDFium reports |
|---|---|---|---|
| `f01` | `pipe-` typed by the author, a real hyphen | `pipe-` (U+002D) | `pipe` + **U+0002** |
| `f02` | `projection`, hyphenated automatically by Typst | `projec` + **U+00AD** (soft hyphen) | `projec` + **U+0002** |

So PDFium reports a **hard hyphen and a soft hyphen as the same character**, U+0002, and
`is_hyphen()` says only *that* a character is a hyphen, never *which*. An independent extractor
keeps them apart; ours cannot.

**Why this matters more than it looks.** Three separate parts of the design assume the
distinction exists:

- D13.4 gives `SoftHyphen` its own ledger reason - "a soft hyphen, U+00AD, removed by
  normalisation `N`". **That reason can never fire from a PDFium stream**, because U+00AD never
  arrives. Left alone, soft hyphens would survive into the EPUB as U+0002 control characters, and
  the budget for removing them would sit permanently unused - the same shape as the `OverdrawDedup`
  gap from item 1.2.
- PIPELINE §369 requires that a genuinely hyphenated compound broken at its real hyphen
  (`Nord-Süd-Achse`, `E-Mail-Adresse`) is **not** rejoined, while a typesetter's soft hyphen **is**.
  That is precisely the distinction PDFium has erased, so Phase 3's dehyphenator cannot make it
  from the character alone and will have to fall back on the dictionary check.
- Item 1.4's note in `PROGRESS.md` said PDFium reports a line-break hyphen "not as U+002D". True
  but incomplete: it is not U+00AD either, and the two cases are indistinguishable downstream.

**What recovers it.** The content stream. `lopdf` access to the `Tj`/`TJ` operands gives the actual
byte the font was asked to draw, which distinguishes the two - and that is the *same* mechanism the
`OverdrawDedup` gap needs. Two open items now point at one piece of work, which makes it worth
doing properly in Phase 2 rather than patching twice.

**The test's own decision.** The three hyphen forms (U+0002, U+00AD, U+002D) are folded to one
before comparison. Test 1.18 asks whether any *text* went missing; re-reporting an encoding
difference as a lost word would make it a worse test of that, and the encoding difference is
recorded here instead. Without the folding it fails on one word out of ~200 and says nothing useful.

**Two decisions about how the test runs.**

1. **A cargo feature, not a skip attribute.** The plan says to run this "only when `pdftotext` is
   on PATH". CLAUDE.md bans marking tests as skipped and `xtask ci-lint` enforces it, on the
   grounds that a skipped test reads as a green one - and a test that silently passes when its
   oracle is absent is that same failure in another costume. The whole file is
   `#![cfg(feature = "poppler-oracle")]`: off, it does not exist and claims nothing; on, it must
   find the binary or fail. A `poppler-oracle` CI job installs `poppler-utils` and turns it on,
   following the `epubcheck` job's pattern. `ci-lint` flagged this file's own prose about the
   banned attribute, as it did in Phase 0; reworded rather than exempted, because the exemption
   list is exactly two files and a test asserts that.
2. **`f02` was added to the plan's `f01`.** `f01` is one column of prose and both extractors walk
   it identically - it agrees on the first run and proves little. `f02` has two columns and
   automatic hyphenation, and it is the one that found the U+0002 collapse. A differential test
   that only ever agrees is not yet a test.

Also worth recording: the oracle here is Xpdf's `pdftotext` 4.00, not Poppler's. Poppler forked
from Xpdf, so it is still an implementation independent of PDFium, which is what the test needs;
CI installs `poppler-utils`, so both are exercised across environments.
Evidence: `cargo nextest run --workspace` - 65 passed; `--features poppler-oracle` - 46 passed
in `oc-pdf`, both oracle tests green.
Affects: D13.4 `SoftHyphen`, D15, PIPELINE §369, R9 §B.5, IMPLEMENTATION_PLAN Phase 1 test 1.18,
`crates/oc-pdf/tests/oracle.rs`, `crates/oc-pdf/Cargo.toml`, `.github/workflows/ci.yml`.

## 2026-09-10 · Image extraction was O(n squared) in page count · Phase 1
Context: found while sizing the 200-page document for item 1.12's cancellation test, which is the
first time anything in this project opened a document with more than three pages.

**The measurement.** `page_images` over documents of increasing size, before the fix:

| pages | per page |
|---|---|
| 50 | 78 us |
| 100 | 158 us |
| 200 | 303 us |
| 400 | 564 us |

Per-page cost doubles as the page count doubles - the signature of a quadratic loop. At the
`max_pages` limit of 3 000 it extrapolates to seconds per page, which is one to two orders of
magnitude past acceptance criterion A1.6's budget of 0.15 s/page.

**The cause.** `pdfium::images::page_id` resolved a page index by calling
`lopdf::Document::get_pages()` and taking the nth entry. `get_pages` walks the entire page tree and
builds a fresh `BTreeMap` every call, so a 3 000-page book built that map 3 000 times. Added in
item 1.5, where every fixture had one or two pages and the cost was invisible.

**The fix.** `PdfiumDoc` reads the ordered page ids once at open time. After:

| pages | per page |
|---|---|
| 50 | 16.8 us |
| 100 | 19.8 us |
| 200 | 18.2 us |
| 400 | 15.6 us |

Flat, and 36x faster at 400 pages.

**The regression test asserts a ratio, not a duration.** `per_page_extraction_cost_does_not_grow_
with_page_count` compares per-page cost at 100 and 800 pages and fails above 4x. An absolute
timing assertion on a shared CI runner measures the runner; a ratio taken in one process cancels
most of that noise. Measured after the fix the ratio is about 0.9 - slightly *better* at the larger
size, because fixed costs amortise - and before the fix it was 7.2, so 4 sits far from both.

The general lesson is worth keeping: **every fixture in this project has one or two pages**, so
nothing before this could have caught a per-page cost that depends on the document. Any future
per-page work should be measured at scale before it is believed, and `oc_testkit::handmade::
many_pages` is now the tool for it.
Evidence: `cargo nextest run --workspace` - 66 passed.
Affects: A1.6, IMPLEMENTATION_PLAN Phase 1 detail 4, `crates/oc-pdf/src/pdfium/{doc.rs,images.rs}`,
`crates/oc-pdf/tests/scaling.rs`.

## 2026-09-10 · Cancellation, and two tests that are deterministic instead of one that races · Phase 1
Context: item 1.12 implements `oc-core::{cancel, progress}` and `openconvert::control` against
test 1.19.

1. **`Cancel` is an `Arc<AtomicBool>` behind a named type, not a channel.** §2.3 requires the flag
   to be polled at stage boundaries and inside every per-page loop, and Phase 12's UI sets it from
   another thread while the loop runs. A relaxed atomic read is cheap enough to do between every
   page of a nine-hundred-page book; a channel receive in the middle of a page loop is not.
   `Release`/`Acquire` rather than `Relaxed`, so anything the cancelling thread did first is
   visible to the thread that observes the flag. Phase 14 adds `AbortCause` to the same flag
   rather than a second mechanism (§14.6), and nothing here has to change for that.

2. **Test 1.19 is deterministic, and the obvious way to write it is not.** "Setting the cancel flag
   during a 200-page ingest" invites a thread that sleeps and then cancels - which works only if
   the loop is still running when the sleep ends. Measured: a 3 000-page dump takes 0.53 s, so 200
   empty pages take single-digit milliseconds, and such a test would pass or fail on how fast the
   machine is. Instead the cancel is fired **from inside the loop's own progress callback**, after
   page 3 of 200. That pins the moment exactly and asserts the property that actually matters: the
   loop stopped at page 4, not at page 200, so the flag is read *between* pages rather than once
   before them. A companion test runs the same loop uncancelled and asserts all 20 pages are
   written, without which the first test would pass against a loop that always stopped at four.

3. **The control channel is tested against a byte slice, not a process.** The remaining link is
   stdin NDJSON to flag. An end-to-end test - spawn the binary, write `{"t":"cancel"}`, hope the
   flag is set before the work finishes - is a race by construction, and at 0.53 s for 3 000 pages
   it is a race the test would often lose. So the reader loop is `drain<R: BufRead>`, `listen` is
   the three lines that put it on a thread over stdin, and `drain` is tested directly. Both halves
   of 1.19 are deterministic; the join between them is three lines with no branch in it.

4. **Unknown control messages are ignored, not fatal.** A supervisor from a newer UI may send a
   message this engine does not know. Refusing to work because of it would turn a
   forward-compatible protocol into a brittle one, so `parse` returns `None` for anything
   unrecognised, malformed or blank, and the reader carries on.

`Outcome::{Completed, Cancelled}` is a separate type from `Result` because being cancelled is not a
failure - it is the answer the user asked for, and it exits 3 rather than 1 (§2.4).
Evidence: `cargo nextest run --workspace` - 75 passed.
Affects: D13.2, IMPLEMENTATION_PLAN §2.3, §2.4, Phase 1 test 1.19, Phase 14 §6,
`crates/oc-core/src/{cancel.rs,progress.rs}`, `crates/openconvert/src/{control.rs,cmd_dump_stage.rs}`.

## 2026-09-10 · VD-d closed: PDFium composites, and the plan's proposed reference could not have said so · Phase 1
Context: the last Phase 1 item. The plan's Failure-modes section asks for a ten-fixture spike
comparing `get_processed_image()` against "a `pypdfium2` reference in `eval/`", with the image
policy chosen from the result.

**The proposed reference is not independent, so the comparison could not answer the question.**
`pypdfium2` wraps *the same PDFium library*. It is a different binding, not a different
implementation. Two bindings agreeing tells us that `pdfium-render` and `pypdfium2` both call the
same C function correctly and nothing whatever about whether the compositing is right. Building
that harness would have produced a green result that meant nothing.

**What was done instead: known-answer tests.** We author the fixtures, so we know what the
composited pixels must be. `h09`'s soft mask is written as "first half of the samples 0, rest 255"
by `oc_testkit::handmade`, so the top four rows of an 8x8 image must come back fully transparent
and the bottom four fully opaque. That is a fact about the file, not about a second library, and it
is falsifiable in both directions - a decoder that ignored the mask, and one that inverted it, both
fail it.

**The answer: `get_processed_image()` composites correctly.** Measured on PDFium `chromium/7881`:

| case | fixture | result |
|---|---|---|
| soft mask (`/SMask`) | `h09_image_smask` | **applied**, exact alpha layout |
| stencil mask (`/ImageMask`) | `h14_stencil_mask` | **applied**, correct polarity |
| indexed colour | `h15_indexed_colour` | **resolved** through the palette |
| DeviceGray -> RGB | `h05_invisible_layer` | expanded, 235 -> (235,235,235) |
| inline image (`BI/ID/EI`) | `h10_inline_image` | decodes with no special case |
| full-page scan | `h05`, `f03` | decodes |
| tiny ornament | - | geometry only, covered by `image_kind_covers_every_arm` |
| rotated image | - | `/Rotate` is page-level; covered by test 1.5 |
| CMYK JPEG | **not covered** | needs a real CMYK JPEG; `image` 0.25 does not encode one |
| 1-bit CCITT | **not covered** | no CCITT G3/G4 encoder available |
| JPX (JPEG 2000) | **not covered** | no JPEG 2000 encoder available |

**So the image policy is: use `get_processed_image()`, and do not write our own compositing.**
`get_raw_image()` stays reachable for the debug flag the plan reserves. Phase 4 can assume masks,
palettes and colour spaces are already resolved by the time it sees pixels.

**The three uncovered formats are honest gaps, not silent ones.** All three are decoded by PDFium's
own codecs rather than by anything we wrote, and all three are cases where a failure would be a
decode error rather than a wrong picture - which `image_bytes` reports as `PdfError::Page`. The way
to close them is a real-world sample in the Phase 7 corpus rather than a synthetic fixture we
cannot honestly build. Recorded as such.

**A trap found on the way.** The stencil-mask test asserted the wrong polarity on its first run and
PDFium was right: PDF 32000-1 §8.9.6.2 says a sample of **0 paints** with the current colour and a
sample of **1 leaves the page unchanged** - the opposite of the intuition that a set bit means ink.
Both the fixture and the test now say so explicitly, because it is exactly the kind of thing that
gets "fixed" in the wrong direction later.

`PdfDoc::image_bytes` is the deliverable: the compositing path Phase 4 and Phase 5 will call. It
checks `max_image_pixels` against the *declared* dimensions before decoding, which matters more
here than in `page_images` - this is the one entry point that actually allocates for an image.
Evidence: `cargo nextest run --workspace` - 82 passed.
Affects: VD-d (**closed**), D13.11, IMPLEMENTATION_PLAN Phase 1 detail 4 and Failure modes,
Phase 4 image policy, `crates/oc-pdf/src/{images.rs,inspect.rs,pdfium/doc.rs}`,
`crates/oc-pdf/tests/smask_spike.rs`, `crates/oc-testkit/src/handmade.rs`.

## 2026-09-10 · A1.6 measured on a 301-page born-digital book · Phase 1
Context: the last unmeasured Phase 1 acceptance criterion. A1.6 asks for wall clock <= 0.15 s/page
and peak RSS <= 250 MB on a 300-page born-digital book.

**The subject.** No 300-page fixture existed - every fixture in the corpus has one or two pages,
which is how the O(n squared) bug in the previous entry survived. So `f01`'s prose body was repeated
300 times into a Typst source and compiled with `xtask::fixtures::compile_fixture`, giving a
**301-page, 308 KB** document of real Typst output: embedded subset fonts, justified prose, running
headers, page numbers and an outline entry per chapter. Not committed - it is derived from a
committed source in three lines and is a third of a megabyte.

**The measurement.** `openconvert dump-stage ingest`, release build, three runs, peak working set
sampled every 5 ms while the process ran:

| run | total | per page | peak RSS |
|---|---|---|---|
| 0 | 3 047 ms | 10.12 ms | 98 MB |
| 1 | 3 112 ms | 10.34 ms | 99 MB |
| 2 | 3 081 ms | 10.24 ms | 91 MB |

**Both budgets are met with room.** 10.2 ms/page against 150 ms/page is a factor of ~15; 98 MB
against 250 MB is a factor of ~2.5. And the figure is pessimistic: `dump-stage ingest` does
extraction *plus* canonical-JSON serialisation of every glyph, font, image and ledger entry, which
is strictly more than ingestion alone.

**The caveat, stated rather than buried.** This machine is not D9's reference machine L (4c/8t AVX2
x86, 16 GB), so these numbers are indicative rather than the official sign-off. The margin is wide
enough that a slower reference machine is unlikely to change the verdict, but the criterion says
machine L and this is not it. Phase 7's benchmark harness is where it gets measured properly.

**No wall-clock assertion was added to the test suite.** A duration assertion in the fast tier
measures the CI runner, which is the argument `tests/scaling.rs` already makes; the standing guard
against the regression that actually threatens this budget - per-page cost growing with document
size - is that file's ratio test.
Evidence: measured 2026-09-10 on the development machine, release profile, PDFium `chromium/7881`.
Affects: A1.6, D9 reference machine L, Phase 7 benchmarks.

## 2026-09-13 · The conservation checker: three deviations from the sketch · Phase 2 item 2.1
Context: `oc-core::ledger_check` is the first thing Phase 2 builds, because the plan's own failure-mode
note says to build the checker before the transformations so every transformation is born under it.
Three things in the plan's sketch could not be implemented literally.

**1. `check_invariants` takes histograms and a running account, not two `Doc`s.**
The sketch is `check_invariants(before: &Doc, after: &Doc, ledger: &LedgerDelta, kind, reasons, c0)`.
`Doc` does not exist yet and will not until Phase 4, and the quantity the law is stated over is
`C(D)` — a histogram — not the document. So the signature is
`check_invariants(before: &CharHistogram, after: &CharHistogram, delta: &LedgerDelta,
decl: StageDecl, totals: &mut ReasonTotals)`. `kind` and `reasons` collapse into `StageDecl`, which is
the same pair the `Stage` trait declares (ARCHITECTURE §3.3) and cannot be widened by the stage that
is being checked. `c0` becomes `ReasonTotals`, because **I-4 is cumulative** — "three stages each
taking 3 % under one reason have taken 9 %" — and a function handed only `|C_0|` cannot see that.
The caller computing `C(D)` is not an accident either: only the caller knows which strings are content
text and which are attribute values outside `C` (ARCHITECTURE §5.2).

**2. `Reason::adds()` was wrong, and `LigatureExpand` proved it.**
Phase 1 wrote `adds()` as `matches!(self, Reason::Ocr)` with the comment "the only reason that adds
rather than removes". PIPELINE §4 says each ligature expansion is a **paired Removed + Added entry
under `LigatureExpand`** — it is the case that makes plain multiset equality fail, which is the whole
reason I-1 is stated with both sides. Under the old predicate `LedgerEntry::added(…, LigatureExpand, …)`
tripped a debug assertion, so the one case the invariant exists for could not be written down. Replaced
with `may_add()` (`Ocr | LigatureExpand`) and `may_remove()` (everything but `Ocr`).

**3. I-4 is charged on net loss per reason, not on gross removals.**
Taken literally — "per reason, cumulative chars ≤ budget(s)(r) · |C_0|" with `LigatureExpand` falling
under the 0.001 "other" bound — the deterministic path fails on any Latin book that sets `ﬁ`. The `fi`
bigram alone runs at roughly 0.3 % of characters in English prose, three times the allowance, and the
stage would abort having lost nothing at all: it removed one scalar and put two back. So the charge is
`removed(r) − added(r)`, saturating at zero. This changes nothing for the thirteen reasons that only
remove, and it makes the budget mean what §5.5 says it is for — bounding how much text a stage may
*lose*, not how much churn it may cause. Recorded here rather than as a threshold change because no
number moved: `conservation.budget.other` is still 0.001.

Also settled while writing it: failures are reported I-3 → I-2 → I-1 → I-4. I-3 goes first because a
`Conserving` stage declares no reasons, so "undeclared reason `RunningHeader`" would be true and
useless — the fault is that it wrote to the ledger at all. And `budget_group` returns `None` for `Ocr`:
a bound stated as a fraction of the source text would forbid transcribing a scanned book, which is
what I-6's region scope exists to permit.

Evidence: ARCHITECTURE §5.2–§5.5, PIPELINE §4, IR_SKETCH stage-kind list, D13.4.
Affects: `crates/oc-core/src/{ledger_check.rs,stages/}`, `crates/oc-model/src/ledger.rs`,
IMPLEMENTATION_PLAN Phase 2 Architecture block.

## 2026-09-13 · `N` composes twice, and U+FB05 expands to `st` · Phase 2 item 2.2
Two things about normalisation that the specification does not say and cannot be left implicit.

**`N` needs a trailing NFC, so it is four steps, not three.** ARCHITECTURE §5.1 writes
`N = strip(U+00AD) ∘ expand_ligatures ∘ NFC`. Stripping a soft hyphen can leave a base character
adjacent to a combining mark it was not adjacent to before — `e U+00AD U+0301` becomes `e U+0301` —
so with the composition step only at the front, the output of `N` is not NFC. Two stated requirements
then fail at once: PIPELINE §4 requires every `Run.text` to be NFC, and test 2.3 requires `N` to be
idempotent (a second application would compose what the first left decomposed). Implemented as
`NFC ∘ strip ∘ expand ∘ NFC`, with the trailing pass skipped when nothing was stripped, since a
ligature expansion is ASCII letters and composes with nothing. No behaviour the specification names
changes; the two properties it asserts now hold.

**U+FB05 expands to `st`, not to `ft`.** IMPLEMENTATION_PLAN Phase 2 detail 1 gives the table as
`ﬀ ﬁ ﬂ ﬃ ﬄ ﬅ ﬆ → ff fi fl ffi ffl ft st`. U+FB05 is LATIN SMALL LIGATURE LONG S T; its Unicode
compatibility decomposition is U+017F (long s) + U+0074, and the long s is an orthographic variant of
`s`. Expanding it to `ft` is the classic long-s misreading and would silently turn `beſt` into `beft`
in exactly the eighteenth-century texts where the ligature still appears — a corruption the
conservation law cannot see, because the multiset balances either way. The plan is the lowest
authority in the stack (CLAUDE.md §1) and ARCHITECTURE §5.1 only says "→ ASCII sequences", so `st` it
is. U+FB06 (`st`) is unaffected and expands the same way.

Evidence: Unicode 16 UnicodeData.txt decompositions for U+FB05/U+FB06; ARCHITECTURE §5.1; PIPELINE §4.
Affects: `crates/oc-text/src/normalize.rs`, IMPLEMENTATION_PLAN Phase 2 detail 1.

## 2026-09-13 · PDFium's U+0002 hyphen marker is decoded at extraction · Phase 2 item 2.4
Context: Phase 1 measured that PDFium reports a hyphen drawn at a line break as **U+0002** with
`is_hyphen()` set, and PROGRESS.md carried it into Phase 2 as the biggest open item. Two halves,
and they have different answers.

**The half that had to be fixed now.** `C_raw` is defined as the multiset of scalars *the document
contains* (D13.4). A document contains no U+0002; no reader sees one; and every `Run.text` built
from those glyphs would carry a control character into the EPUB. So extraction resolves the marker
to U+002D — `decode_hyphen_marker` in `pdfium/doc.rs`, guarded on `is_control()` so a flag on a
genuine `-` leaves it alone. This is a backend marker being decoded, not a transformation of the
text, so it happens before `C_raw` is counted and never reaches the ledger. Three tests in
`tests/hyphen_marker.rs` hold the line, including one that asserts *no* extracted glyph is a
control character on `f01` and `f02`.

**The half that stays open.** PDFium collapses U+002D and U+00AD into the same marker, so a soft
hyphen the producer chose to print is indistinguishable from a hard one. U+002D is what the page
prints either way and is the honest answer for `C_raw`, but it means D13.4's `SoftHyphen` reason
fires only for a U+00AD that arrives in the char stream un-printed, and PIPELINE §369's compound-word
rule (`Nord-Süd-Achse` must not be rejoined) still has to find its evidence elsewhere. Recovering
the distinction needs the content stream's `Tj`/`TJ` operands, is a *dehyphenation* input rather than
an extraction one, and therefore belongs to Phase 3 alongside the `OverdrawDedup` count that needs
the same mechanism. PROGRESS.md carries it forward.

Evidence: `f02` breaks `projec-tion` and `reading-order`; test 1.18's `pdftotext` oracle reports
U+00AD for the first and U+002D for `f01`'s `pipe-`, PDFium U+0002 for all three.
Affects: `crates/oc-pdf/src/pdfium/doc.rs`, `crates/oc-pdf/tests/{hyphen_marker.rs,oracle.rs}`,
Phase 3 dehyphenation, PROGRESS.md carried debt.

## 2026-09-13 · Word and line assembly: four choices the specification leaves open · Phase 2 item 2.5
**Hand-made fixtures are h16–h18, not the plan's h07–h12.** The Phase 2 table names `h07` for the
superscript marker, `h08` for letter-spaced text and `h09` for mixed sizes; Phase 1 had already spent
h01–h15 on other things. The plan was written before that happened, so the six Phase 2 fixtures take
the next free numbers. Test names are unchanged — those are the contract.

**The baseline tolerance is a fraction of the *larger* of the two sizes.** PIPELINE §4 step 1 says
"0.3 × font size" and explains that the fraction exists because superscripts pull the baseline; it
does not say whose size. Taken of the smaller one it fails the case it was written for: h18's 7 pt
marker sits 3 pt above its line, `0.3 × 7 = 2.1` rejects it and `0.3 × 12 = 3.6` keeps it. So a
cluster carries the baseline of its largest member as the anchor and compares against
`0.3 × max(cluster size, candidate size)`.

**The 2-means separation test needs a floor, and the floor is a threshold.** The fit is rejected when
the centroid distance over the within-cluster spread falls below `words.gap_separation_ratio_min`.
With a spread of exactly zero — which happens whenever every gap in a cluster is identical, i.e. on
most of the small fixtures — the ratio is infinite and everything separates, including two clusters a
thousandth of a point apart. `words.gap_resolution_pt` (0.05) floors the spread and also gates the
whole fit: PDFium reports geometry to about 0.01 pt and no typographic distinction is made at a
twentieth of a point, so two gap clusters closer than that are one cluster.

**`words.fallback_space_ratio` is 0.25 em, and it is a stand-in.** PIPELINE §4 step 3 says to fall
back to "the font-metric default space width", but `FontInfo` carries no metrics — nothing in the IR
knows how wide this font's space is. 0.25 em is the low end of the base-14 proportional faces (Times
0.250, Helvetica 0.278), and low is the safe side: too small a threshold splits a word, too large a
one merges two. Marked provisional with the real fix named in its evidence — the font's own `/Widths`
entry for the space glyph, once `oc-pdf` exposes it.

Also settled: an inserted space is written at the **end** of the run it follows, including across a
style boundary, so concatenating a line's runs reproduces the line; and `Run::glyph_range` indexes the
assembly order, which `assemble_runs` returns alongside the runs, because the backend's glyph order is
not reading order (Phase 1 item 1.3) and a range into it would not be contiguous.

Evidence: measured on h16/h17/h18 through PDFium `chromium/7881`; PIPELINE §4, R2 §B.3/§B.6, R10 §6.2.
Affects: `crates/oc-text/src/{words.rs,lines.rs}`, `crates/oc-model/src/text.rs`,
`crates/oc-testkit/src/handmade.rs`, `thresholds.toml`.

## 2026-09-13 · Furniture: the "≥ 3 pages" rule, scoped requirements, and a space PDFs already wrote · Phase 2 item 2.6
**The repeat requirement is computed inside the scope it is applied to, and capped at it.**
PIPELINE §5 step 4 says "repetition on ≥ 3 pages or ≥ 20 % of pages, whichever is larger". Read
globally that rule cannot fire on `f01`, which has two pages and whose running header test 2.10 and
acceptance A2.2 both require to be removed; and it defeats the sliding window it sits next to, since a
chapter head on 20 pages of a 300-page book fails a global 20 % bar (60 pages) however perfectly it
repeats inside its chapter. So the ratio *and* the requirement are both computed per scope — global,
odd, even, or the best window of `layout.furniture.window_pages` — as
`max(3, ceil(0.20 × scope)) capped at the scope, floored at 2`. On `f01` the scope is two pages and the
bar is two; on a 20-page window the bar is four; on a 300-page global scope it is still sixty. The
floor of two is the real safety rule and is stated separately as `MIN_PAGES_FOR_EVIDENCE`: a one-page
document gets no furniture detection at all, because "this line appears on every page" is a true
statement about one page and it means nothing.

**An all-numeric band line that fails the progression test is kept, and not reconsidered.**
A constant `3` in the footer band of four pages repeats perfectly and would satisfy every running-foot
rule there is. The arithmetic-progression test is the whole of what separates a page number from a
chapter number (PIPELINE §5 step 6), so failing it ends the matter rather than falling through to
`RunningFooter`. Test 2.12 is exactly this case.

**Justified text already has its spaces, and inventing more gives `It  was  a  dark`.**
Found by test 2.10 on `f01`, not reasoned about in advance. Typst justifies by stretching the space
with `TJ` offsets, so the advance gap *after* a real space glyph is as wide as any inferred space. Two
changes in `oc-text::words`: a space is only invented between two glyphs that are both non-space, and
gap pairs involving a space are excluded from the distribution the 2-means is fitted to — that gap
measures justification stretch, not word spacing, and it is the one gap no space will ever be
inserted into.

**`detect_furniture` takes the document language.** The plan's signature does not. Folding the band
key is the one place casing happens and Turkish pairs its dotted and dotless i its own way (R10 §6.3);
a Turkish running head folded under invariant rules stops matching itself on the next page.

Fixtures h19 (a constant band number), h20 (recto/verso heads plus a one-off) and h21 (a page whose
only line is its running head) are new, and `oc-testkit` grew a `build_pages` helper — cross-page
detection needs several pages and `build` writes one, with six optional features in a fixed object
layout that threading a page count through would complicate for the fourteen fixtures that want one.

Evidence: measured on f01, h19, h20 and h21 through PDFium `chromium/7881`; PIPELINE §5, R2 §B.4,
R10 §6.6, R1 §C.2 #7.
Affects: `crates/oc-layout/src/furniture.rs`, `crates/oc-text/src/words.rs`,
`crates/oc-testkit/src/handmade.rs`, `crates/oc-model/src/text.rs`, `thresholds.toml`.

## 2026-09-13 · PDFium expands ligatures, and h04 was never a ligature fixture · Phase 2 item 2.7
Two findings, one on top of the other, both found by running the conservation law end to end.

**h04 drew `?n`, not `ﬁn`.** `to_winansi` mapped every character outside WinAnsi's byte range to
`?`, and U+FB01 is outside it. The fixture's own comment said the ligature "is written through the
font's own encoding below"; nothing wrote it. Every Phase 1 test that carried h04 through unchanged
passed by carrying through a question mark. It now shows byte 200 with a `/ToUnicode` CMap declaring
that byte to be U+FB01 — how a real typesetter ships a ligature.

**And PDFium expands it anyway.** With a correct CMap in the file, `chromium/7881` hands back `f`,
`i`, `n`. Tried both with `/Differences [200 /fi]` over WinAnsi and with the CMap alone: same answer.
R2 §B.8 states the opposite — "PDFium does not expand ligatures, so `ﬁ ﬂ ﬀ ﬃ ﬄ ﬅ ﬆ` arrive in the
char stream and are mapped explicitly" — and that is not reproduced on this build.

What changes, and what does not:

- **`N` keeps its ligature table.** The contract is about the text, not about which component
  expanded it. Another backend, another PDFium build, or a `/ToUnicode` PDFium declines to honour
  all put the ligature back in the stream, and the unit tests (2.1, and the whole-block test) pin
  the mapping regardless.
- **`LigatureExpand` will rarely fire on PDFium-sourced text.** Its budget is therefore unspent and
  its `Added` side untested by any fixture — which is why the property half of test 2.15 generates
  U+FB00–FB06 directly into the glyph stream rather than relying on a PDF to deliver one.
- **The expansion is invisible to the ledger, like the hyphen marker.** `C_raw` is counted after
  extraction, so a document containing one scalar and a `C_raw` containing two is a backend-fidelity
  gap, not a conservation violation: the law holds from `C_raw` onward and says nothing about the
  step before it. Auditing that step means decoding the content stream against each font's
  `/ToUnicode` — the same mechanism the soft-hyphen distinction and the `OverdrawDedup` count need,
  and it stays with them in Phase 3.
- The end-to-end test asserts what is true either way: the word arrives whole, no U+FB00–FB06 reaches
  the output, and the equation balances whichever component did the expanding.

Also in this item: the pipeline wiring is a **library target on `openconvert`**, not `oc-core`.
ARCHITECTURE §3.1 puts the orchestrator in `oc-core`, and it cannot go there — `oc-core` owns
`thresholds`, every stage crate reads its numbers from it (D17), so `oc-pdf`, `oc-text` and
`oc-layout` all depend on `oc-core` and `oc-core` cannot depend on them without a cycle. The binary
is the one crate allowed to depend on everything. What stays in `oc-core` is everything that does not
need a stage: thresholds, the conservation checker, the stage declarations, cancel, events, exit
codes.

Evidence: `corpus/fixtures/handmade/h04_ligature_fi.pdf` (byte 200, `/ToUnicode <C8> <FB01>`),
`dump-stage ingest` on PDFium `chromium/7881`; R2 §B.8; ARCHITECTURE §3.1, §5.2.
Affects: `crates/oc-testkit/src/handmade.rs`, `crates/openconvert/src/{lib.rs,pipeline.rs}`,
`crates/openconvert/tests/conservation.rs`, R2 §B.8's claim, Phase 3 content-stream work.

## 2026-09-13 · Word-frequency lists: English ships, German and Turkish are a licence question · Phase 2 item 2.9
**Only `en.bin` is committed, and that is a licence finding, not an omission.** PLAN Phase 2 detail 5
says to build EN/DE/TR lists "from CC0/PD text only (Standard Ebooks, DTA plain text,
Wikisource-TR)". Standard Ebooks dedicates its editions to the public domain under CC0 and English
is built from twelve of them. DTA and Wikisource-TR host public-domain **works** under **CC-BY-SA
transcriptions** — a different licence from the one the plan claims for them, and one that is not on
D15's allow-list for a shipped artefact. Choosing a substitute source, or admitting CC-BY-SA data,
is a `DECISIONS.md` change and not a script change; the generator refuses any source whose licence
is not `CC0-1.0`, `PD-US` or `PD`, so the refusal is mechanical rather than a comment. Maintainer
decision taken 2026-09-13: ship the machinery and the English list now, leave DE/TR to a later
`wordfreq.py` run.

The shipped English list is **20 000 words, 232 KB**, not the plan's 200 000 / 1.5 MB. Twelve
Standard Ebooks yield about that many forms above the minimum count of two; `--top-n` is a flag and
the number grows with the source set. Every source, its SHA-256 and its licence are in
`crates/oc-text/src/freq/en.sources.json`, committed next to the blob.

**`dict_hit_rate` returns `Option<f32>`, and the `None` is the point.** `Some(0.0)` means "nothing
here is a word", which is what a page of glyph indices scores and what `broken_text` fires on.
Returning `0.0` for a language with no list would call every German book broken. `oc-pdf`'s
`classify_page` already took an `Option`; it now gets a real value for English.

**Tokens are classified before they are counted.** The obvious tokenizer — runs of letters — makes a
page of glyph indices produce *no tokens at all*, so the rate is unmeasurable on the one input the
signal exists for, and test 2.22 cannot pass. Whitespace-separated tokens are sorted into three
kinds instead: a `Word` to look up; `Undecodable`, which counts in the denominator and never hits,
because a control character where a letter should be is a word that failed to decode rather than
not-a-word; and `NotEvidence` — numbers, bare punctuation, single letters — counted in neither part
of the ratio, so a page of dates is neither broken nor measured.

The blob format is a sorted string table plus a `u32` offset index, binary-searched. No FST, no
perfect hash, no crate: the operation is "does this byte string appear in a sorted list", and a
format anyone can read with a hex editor is one nobody has to trust. Words are stored **folded**, by
the same rule `fold_key` applies, so the Turkish list — when it exists — will hold `ısparta` and a
query for `ISPARTA` will find it.

Evidence: `eval/src/oc_eval/generate/wordfreq.py`, `crates/oc-text/src/freq/en.sources.json`;
Standard Ebooks' public-domain dedication; D15 allow-list; R10 §4.4 row 1.
Affects: `crates/oc-text/src/freq.rs`, `crates/oc-text/src/freq/`, `crates/oc-text/src/stats.rs`,
`eval/src/oc_eval/generate/wordfreq.py`, D15 (open question: DE/TR sources), PLAN Phase 2 detail 5.

## 2026-09-13 · `whatlang` speaks ISO 639-3 and `dc:language` does not · Phase 2 item 2.10
`whatlang::Lang::code()` returns three-letter ISO 639-3 codes — `eng`, `deu`, `tur`. BCP-47 requires
the *shortest* code that exists for a language, and `dc:language` is BCP-47, so a package emitted
straight from the detector would say `eng` and fail EPUBCheck at the very end of a conversion, which
is the worst possible place to find out. `lang.rs` carries a 70-entry 639-3 → 639-1 table, one entry
per language `whatlang` knows, and a test iterates `Lang::all()` asserting every one has a two-letter
tag — so a `whatlang` bump that adds a language fails at `cargo test` rather than at EPUBCheck.

Two entries are macrolanguage judgements rather than lookups: `cmn` (Mandarin) → `zh` and `pes`
(Western Persian) → `fa`, because those are the tags a reading system matches a voice to.

**`lang.block_min_confidence` is new, at 0.60.** PIPELINE §4 step 7 and R10 §6.17 both require "the
top-2 confidence margin" to clear "a fixed floor" without naming the floor. `whatlang`'s `confidence`
*is* that margin, normalised to 0..1, so this is the floor applied to it. Provisional: the number
worth fitting is the one that keeps a monolingual book free of spurious per-block tags on the Phase 7
corpus, and there is no corpus yet.

**Two new Typst fixtures, f04 (German) and f05 (Turkish).** Test 2.19 asks for "three single-language
fixtures" and only English existed. Both pages are written for the fixture rather than quoted, so
nothing third-party is redistributed (D18, TEST_CORPUS §1.1), and both are ordinary prose rather than
sentences chosen to be easy to detect. Turkish is not filler: `ı`, `İ`, `ğ` and `ş` have to survive
extraction as *themselves*, and a second test asserts they do — if they arrive folded or
transliterated, `whatlang` may still say Turkish while every lookup key in the pipeline is wrong.
Hand-made fixtures could not carry them: `to_winansi` writes Latin-1 bytes and Latin-1 has no `ğ`,
`ş` or `ı`, so this had to be a Typst fixture with a real font.

Evidence: `whatlang` 0.18 `src/lang.rs`; measured on f01/f04/f05 through PDFium `chromium/7881`.
Affects: `crates/oc-text/src/lang.rs`, `corpus/fixtures/typst/f0{4,5}_*.typ`, `thresholds.toml`,
`xtask/src/fixtures.rs` (the fixture count assertion).

## 2026-09-13 · The snapshot found a word-splitter, and `words.min_space_ratio` closed it · Phase 2 item 2.11
Test 2.21's first run produced this, and it is why the row asks for a snapshot rather than a
predicate:

> `It was a dark and stormy night; the rain fell in torrents, except at o ccasional`
> `inter vals, when it was che cke d by a violent gust of wind which swept up the`

Every assertion in the phase still passed. The conservation law was green — a space is whitespace,
outside `C`, so splitting every word on the page conserves the multiset exactly. The furniture tests
passed, the language test passed, the dictionary hit rate merely dropped from 0.97 to 0.83 and stayed
far above the `broken_text` floor. Nothing but a human reading the output could see it.

**The cause.** `f01` is justified text whose real spaces are drawn as space glyphs, and item 2.6
excluded gap pairs involving a space from the sample (they measure justification stretch, not word
spacing). What is left to fit is *only intra-word kerning* — and 2-means always returns two clusters.
It duly separated 0.0 pt from about 0.5 pt with a within-cluster spread of nearly nothing, which the
separation ratio scored as perfect, and thresholded at a quarter of a point. Every kern pair in the
book became a word boundary.

**The fix, and why it is not a bigger separation ratio.** The separation test is scale-free by
construction: 0.0 against 0.5 separates exactly as well as 0.0 against 5.0, so no value of
`words.gap_separation_ratio_min` distinguishes them. What was missing is an absolute floor —
`words.min_space_ratio` (0.15 em), applied as `max(fitted, floor × size)` by the caller. Nothing
narrower than that is a word space, whatever the fit says. The narrowest base-14 space is Times'
0.25 em and justification compresses it to perhaps 0.6 of that; f01's intra-word kerning runs at
0.05 em. The floor sits between, with margin on both sides.

Also settled here: `dump-stage text` cannot stream from page one the way `dump-stage ingest` does.
`dc:language` is detected over the whole body and `C_0` is not final until every page has been
through `N`, so the stage runs to completion and the pages are written from the result. The memory
is bounded and far below the ingest dump's, since a book's runs are a fraction of its glyphs — one
string and one box per run where there was one of each per character. Recorded rather than fixed:
streaming would mean two passes, and the second one would read what the first wrote.

Evidence: `crates/openconvert/tests/snapshots/dump_text__dump_stage_text_f01_page0.snap`, before and
after; PIPELINE §4 step 3; R10 §6.2.
Affects: `crates/oc-text/src/words.rs`, `thresholds.toml`, `crates/openconvert/src/dump_text.rs`,
`crates/openconvert/src/cmd_dump_stage.rs`.

## 2026-09-13 · The first CI run, and what it found · CI
Context: `ci` triggers on `push: branches: [main]` and on pull requests. Phases 0, 1 and 2 were all
developed on `phase/00-bootstrap`, which is neither — so until `main` was pushed today, **the CI
workflow had never executed once**. Three Definition-of-Done entries say "Linux and macOS are CI's
job"; that sentence was never cashed. Five of eight jobs failed on the first run.

**Four failures, one cause: the Tauri crate cannot build inside `--workspace` on a bare runner.**
`test (ubuntu)`, `no-network` and `lint` died on `glib-sys` — no GTK or WebKit on the image.
`test (macos)` and `test (windows)` died on `resource path bin/openconvert-<triple> doesn't exist`:
`tauri-build` resolves `externalBin` at build time, the sidecar is staged by
`xtask stage-sidecars`, and CI never ran it. It builds locally only because a Windows sidecar and a
built `ui/dist` happen to be sitting in the git-ignored directories from Phase 0.

The fix is not to install a GUI toolchain in five jobs across three operating systems. **The Tauri
crate has no Rust tests at all** — test 0.22, the engine handshake, is a Vitest test in
`apps/desktop/ui` and runs in the `ui` job, which was green — so its only assertion is "it
compiles". That assertion is bought once, on Linux, in a new `desktop` job that installs the
toolchain, builds `ui/dist`, builds the engine, stages the sidecar and runs clippy over the crate.
The engine jobs say `--exclude openconvert-desktop`. `cargo fmt --all` and `cargo deny` still cover
it; neither builds it. Phase 12 owns the UI and Phase 15 owns cross-platform packaging, and that job
is where the matrix goes when they arrive.

**`deny` failed on argument order, in all three of its steps.** `--config` and `--all-features` are
*global* options in cargo-deny and must precede `check`; `cargo-deny-action` appends its
`arguments` after the subcommand, so `check --config deny.tools.toml … licenses bans sources` was
parsed with `licenses` as a subcommand — `unrecognized subcommand 'licenses'`. The action also runs
in a musl container that cannot honour `rust-toolchain.toml`
(`override toolchain '1.98.1-x86_64-unknown-linux-musl' is not installed`). Replaced with
`taiki-e/install-action` plus direct invocations on the host toolchain, verified locally.

**Two jobs and one step assert things that do not exist yet**, and would have gone red the moment
`test` went green: `epubcheck` calls `xtask fetch-epubcheck` and `epubcheck-corpus` (Phase 5),
`dom-checks` needs `tests/dom` (Phase 6), and `no-network`'s last step calls
`xtask assert-no-net-deps` (Phase 14). All three are now `if: false` with a comment naming the phase
that turns them on — the idiom this file already uses for `nightly-placeholder`. A job that reports
red for a reason unrelated to the code under review teaches everyone to ignore the colour, which is
worse than an absent job. The socket ban is meanwhile enforced by `deny.toml`'s `wrappers` rule in
the `deny` job, which is a build-time property and not the weaker of the two.

**The lesson worth keeping:** a per-phase branch that CI does not watch accumulates exactly this.
Phase 3 onward runs on branches that open a pull request, so `ci` fires on every push.

Evidence: run 34755670348 on `main`, 2026-09-13.
Affects: `.github/workflows/ci.yml`, `docs/TEST_MATRIX.md` (the CI-jobs table), PROGRESS.md.

## 2026-09-13 · A non-embedded base-14 font makes glyph boxes host-dependent · CI
Found by the third CI run: `dump_stage_ingest_snapshot_h01` passed on Windows and macOS and failed
on Ubuntu. The same fixture, the same PDFium build, different numbers:

| field | Windows / macOS | Ubuntu |
|---|---|---|
| `bbox.x1` (`A`) | 80.02 | 79.85 |
| `bbox.y0` | 91.41 | 91.38 |
| `loose_bbox.y0` / `.y1` | 89.14 / 102.53 | 88.66 / 102.69 |
| `origin` | 72.00, 100.00 | 72.00, 100.00 |

h01 draws base-14 **Helvetica and does not embed it**, which is what a great many real producers
do. The PDF specification says a viewer substitutes in that case, so the *outline* of every glyph —
and therefore its inked box and its ascent and descent — is whatever font the host offered. The
**advance is identical on both**, because the widths come from the PDF's own metrics rather than
from the substitute. That is why `origin` matches to the hundredth of a point across three
operating systems while `bbox` does not.

**Three consequences, and only the first is a fixture problem.**

1. *This snapshot* now masks `bbox` and `loose_bbox` and nothing else, and asserts in the same test
   that each glyph still sits on the document's baseline and that its inked box is inside its
   advance box — so the mask hides a host difference rather than a fault. Box correctness has never
   been this test's job anyway: tests 0.8, 0.8a, 1.5 and 1.6 each compare a document against
   *itself* on one host and are immune to substitution by construction.
2. *The fixture stays as it is.* Embedding a font would make the numbers stable and would also make
   h01 stop representing the case it exists to represent. A fixture for "the producer relied on
   base-14 substitution" is worth having precisely because the real world is full of them.
3. **D13.8's determinism contract does not hold for such documents, and cannot.** Any decision keyed
   on an inked box — block segmentation, drop-cap detection, heading geometry — may differ between
   hosts on a base-14 document, and no amount of code fixes that; the substitution happens below us.
   The mitigation is to prefer the advance box and the origin, which *are* stable, wherever a rule
   has the choice. Phase 2's word assembly already reads `loose_bbox.x`, whose difference here is
   0.02 pt against a `words.min_space_ratio` floor of 1.5 pt at 10 pt — three orders of magnitude of
   margin. **Phase 3 has the choice far more often and should make it deliberately**; carried into
   PROGRESS.md.

Evidence: run 34757445462 on `main`, job `test (ubuntu-latest)`.
Affects: `crates/oc-pdf/src/dump.rs` and its snapshot, D13.8, Phase 3 layout rules.

## 2026-09-13 · VD-b closed: the `hyphenation` crate's patterns are not ours to redistribute · Phase 3
Context: VD-b blocks Phase 3. The crate's own licence (Apache-2.0 OR MIT) was already confirmed; what was
open is the licence of the `hyph-utf8` **pattern files** it bundles, and whether DE, TR and EN patterns
are present at all. `hyphenation = { version = "0.8" }` has sat in the workspace dependency table since
§1.2 of the plan, unused by any crate.

Decision: **do not depend on `hyphenation`.** It is removed from the workspace dependency table and
added to the `deny` list in both `deny.toml` and `deny.tools.toml`, with the reason inline. v1 needs no
Knuth-Liang patterns: PIPELINE §7 dehyphenates with four deterministic tiers plus the committed
classifier, and hyphenating *for* the reader is the reading system's job in a reflowable EPUB, never
ours. If a later phase ever wants patterns, take `hyph-de-1996` and `hyph-en-gb` from the upstream master
files with their headers intact; Turkish and `en-us` need their own decision first.

Evidence: `hyphenation 0.8.4` (`static.crates.io`, sha256
`bcf4dd4c44ae85155502a52c48739c8a48185d1449fff1963cffee63c28a50f0`, matching the crates.io index
`cksum`), unpacked and read 2026-09-13.

1. **The languages are present.** `dictionaries/` carries `de-1901`, `de-1996`, `de-ch-1901`, `en-gb`,
   `en-us` and `tr`, all `.standard.bincode`, alongside 70-odd others; `patterns/` carries the
   corresponding `.pat.txt` sources. So the "are they there" half of VD-b answers yes.
2. **The crate ships them stripped of their licence headers.** Every `patterns/*.txt` file in the crate
   begins with its first pattern — `grep -li 'licen|copyright' patterns/*.txt` matches exactly two files,
   `hyph-ca.ext.lic.txt` and `hyph-hu.ext.lic.txt`, which are standalone licence texts for the *extended*
   Catalan and Hungarian patterns. The upstream masters all carry a `% licence:` block; these copies do
   not.
3. **The crate disclaims them in its own README** (§License): "`hyph-utf8` hyphenation patterns © their
   respective owners; see their master files for licensing information." The dual-permissive field on
   crates.io covers the Rust code and says nothing about the data — which is exactly why a licence
   scanner cannot catch this and why the ban, not the allow-list, is where it is enforced.
4. **Upstream, the three languages do not answer the same way** (`hyphenation/tex-hyphen` at
   `49706f9`, `hyph-utf8/tex/generic/hyph-utf8/patterns/tex/`):
   - `hyph-de-1996.tex` — **MIT**, © 2013–2018 Deutschsprachige Trennmustermannschaft. On D15's list.
   - `hyph-en-gb.tex` — **MIT**, © 1992–2016 Wujastyk & Toal. On D15's list.
   - `hyph-en-us.tex` — a **bespoke permissive notice** ("Copying and distribution of this file, with or
     without modification, are permitted in any medium without royalty provided the copyright notice and
     this notice are preserved"), © 1990–2005 Gerard D.C. Kuiken. Permissive in substance, but it is not
     an SPDX identifier and D15's allow-list is a list of identifiers.
   - `hyph-tr.tex` — **LPPL 1.0 or later**, © 1987 Pierre A. MacKay, 2008/2011 TUG. **Not on D15's
     list**, and Turkish is one of v1's three languages.
5. **And the compiled dictionaries fold in worse.** `hyph-ca.ext` is LGPL-3.0+/GPL-3.0+ (Jaume Ortolà,
   Riurau Editors) and `hyph-hu.ext` is MPL-1.1/GPL-2.0/LGPL-2.1 (Nagy Bence). Depending on the crate at
   all puts that data in the build, and D15 bans the GPL family in a shipped artefact outright.

Affects: VD-b (**closed**), D15, `Cargo.toml` §1.2, `deny.toml`, `deny.tools.toml`,
`docs/LICENSE_AND_DEPENDENCIES.md` §2.1 and note 2, IMPLEMENTATION_PLAN Phase 0 VD table, Phase 3.

## 2026-09-13 · Docstrum's between-line vector is measured between boxes, not centroids · Phase 3
Context: block segmentation, PIPELINE §6 step 1. The literal reading of Docstrum — nearest-neighbour
vectors with the between-line band [45°, 135°] — was implemented over line *centroids*, since `text` has
already done the within-line half and a line is what is left to link.

Decision: measure the vector between the two line **boxes** instead: horizontal separation (zero when
they overlap on x, the gap when they do not) against the difference of their vertical middles. The angle
band and the 1.3 multiplier are unchanged and still do the work the paper gives them.

Evidence: on `f01` page 0 the centroid reading produced seven blocks where the page has three, and every
spurious boundary was in the same place — before a paragraph's last line. A short last line's centroid
sits far to the left of the full-measure line above it (211.1 pt versus 144.6 pt on `f01`), so the
centroid-to-centroid vector comes out at 169°, outside the between-line band, and the line that ends
every paragraph is cut into a block of its own. The cross-check saw it immediately: best IoU 0.038 on
block 0, which is exactly what the whitespace cover is there to catch. With the box reading, `f01`
segments into 3 + 1 blocks and every IoU is 1.0.

The mistake is not in the paper. Docstrum's between-line neighbours are *characters* — a glyph and the
glyph directly beneath it — so its vector is vertical whenever one line sits under another, whatever the
two lines' widths. A centroid is a property of a line; Docstrum never had one.

Affects: `crates/oc-layout/src/blocks.rs`, test 3.1, PIPELINE §6 step 1.

## 2026-09-13 · `segment_blocks` takes a page, not a line slice · Phase 3
Context: IMPLEMENTATION_PLAN Phase 3 gives the signature
`segment_blocks(lines: &[Line], t: &Thresholds) -> (Vec<Block>, SegmentationAgreement)`.

Decision: take a `LayoutPage` — the page reference, its size, and the surviving lines each paired with
its text. Returned `Block`s are otherwise unconstructible: `Block.page` is a `PageRef` and `BlockId` is
derived from `page_index ‖ bbox ‖ first 64 chars` (D13.3), so both the page index and the line text have
to be in scope. The line text is carried on the input rather than recomputed because `furniture` has
already filtered the lines, and a `Line`'s run indices point into a page's runs that this crate does not
hold.

Same for the page's own box: the whitespace cover needs a bound to search inside, and it uses the text
area rather than the page, since a page's margins are the largest white rectangles on it by a wide
margin and they separate nothing.

Affects: IMPLEMENTATION_PLAN Phase 3 Architecture, `crates/oc-layout/src/blocks.rs`,
`crates/openconvert/src/pipeline.rs` (`PageInput`/`TextPage` gain `width_pt`).

## 2026-09-13 · f02 had never had two columns, and a line had never been split at a gutter · Phase 3
Context: Phase 3 tests 3.2 and 3.3 are the two-column reading-order assertions, over
`f02_two_column.pdf`. Measuring the fixture before asserting on it turned up two separate faults, one in
the fixture and one carried forward from Phase 2.

**The fixture.** `f02` sets `columns: 2` on a 297 mm page with 20 mm margins — 51 lines to a column — and
carries 17 lines of body text. Typst fills the first column before the second, so every line of it was in
the *left* column and the right column was empty. Every "two column" assertion over it passed vacuously,
including the reading-order one: `The left column continues` did precede `The right column begins`,
because both were in the same column, one above the other.

Decision: give the fixture enough text to fill a column, and put an explicit `#colbreak()` before the
section that begins the right column. The page size, margins, gutter, header, footer and floating title
are unchanged, so the furniture evidence and the hyphenated line breaks the Phase 1 and 2 tests read are
all still there. The column break is explicit rather than by overflow so that a line-breaking difference
between Typst versions cannot silently move the boundary back to where it was. Page 0's `visible_chars`
goes 786 → 3038 in the inspect snapshot; page 1 is untouched.

Shrinking the page instead was tried first and rejected: at 110 mm the 20 mm margin is 18 % of the page
height, the running header falls outside `layout.furniture.band_ratio`'s 8 % band, and the fixture stops
being a document furniture detection can see at all.

**The line split.** With a real right column, `text` assembled the two columns' lines into single lines
spanning the page — the carry-forward recorded in PROGRESS.md as item 2, "`text` clusters a line by
baseline alone". A line that spans a gutter is not repairable downstream: it has one bounding box across
both columns, one indent and one right gap, and Docstrum then links it to both columns at once. On `f02`
this produced blocks 486 pt wide containing lines from both columns.

Decision: split a baseline cluster at any gap wider than `text.line_split_gap_em` (1.2 em), and break a
*run* at the same gap so that a run never spans one either. This is not column detection — it makes no
claim about where the columns are — it is a refusal to assert that two things the page kept 16 pt apart
are one line. The inserted space is whitespace and outside `C`, so nothing is ledgered and the
conservation law is untouched.

Evidence: `f02` page 0, gutter measured at 290.7–306.7 pt (16.0 pt, 1.7 em at 9.5 pt) against a widest
justified word space well under 1 em. After the split, column detection finds exactly one gutter,
`ColumnLayout` is `[(56.7, 290.7), (306.7, 542.6)]`, and the twelve blocks of page 0 each sit in one
column.

Affects: `corpus/fixtures/typst/f02_two_column.typ`, its inspect snapshot, `crates/oc-text/src/lines.rs`,
`crates/oc-text/src/words.rs`, `thresholds.toml` (`text.line_split_gap_em`), PROGRESS.md carry-forward 2
(**closed**), tests 3.2 and 3.3.

## 2026-09-13 · A gutter is a valley with text on both sides of it · Phase 3
Context: PIPELINE §6 step 2 defines a gutter as a valley in the x-projection that is wide enough, empty
enough, and empty over enough of the text height. Implemented literally, that admits two things that are
not gutters: the blank lower half of a short column, and the page's own outer margin. On `f02` the first
one fired — the right column ends at 42 % of the page height, so every strip below it is tall and empty,
and the detector reported a 252 pt "gutter" running to the right edge of the text.

Decision: add the condition that makes a gutter a gutter — **text on both sides of it, within the same
vertical window that qualified it**. The blank half of a short column has text to its left and nothing to
its right; the outer margin has text on one side only. Both are rejected by the same clause, and no new
threshold is needed.

The test is applied per one-point strip rather than per merged run, deliberately: the strips of the real
gutter and the strips of the blank half of the column are adjacent, so a run-level test would ask for
text to the right of the *page* and throw away the real gutter with the false one.

Evidence: `columns::the_blank_half_of_a_short_column_is_not_a_gutter`, and `f02` page 0, where the
reported gutter goes from (290.7, 542.7) to (290.7, 306.7).

Affects: `crates/oc-layout/src/columns.rs`, PIPELINE §6 step 2, tests 3.2 and 3.3.

## 2026-09-13 · The column retry compares hypotheses instead of assuming the narrower one · Phase 3
Context: PIPELINE §6 step 4 — "if continuity breaks on more than 30 % of pages, the column hypothesis is
wrong → re-run with k−1 columns. Bounded to two retries, then accept and warn."

Decision: keep the rule and add one condition — **the re-run has to read better.** A narrower hypothesis
is adopted only when its continuity break rate is strictly lower than the one it replaces.

Evidence: `f02` is a genuine two-column document of two pages. It has exactly one page boundary, and that
boundary falls at the end of a sentence, because the section ends there. The literal rule measures a 100 %
break rate, downgrades a correct two-column page to one column, and interleaves it — the very failure test
3.2 exists to catch. Under the comparison, the one-column reading of `f02` breaks the same boundary for
the same reason, is not better, and is not adopted.

The condition costs one extra layout pass and it is what makes the retry evidence rather than a reflex: a
break rate is a statement about a document, and a document's own alternative reading is the only baseline
available for it.

Also recorded: the fixture the plan names for this test (`h13`, "false gutter") is `h13_outline`, which
Phase 1 spent on the outline walk. Following the rule PROGRESS.md sets out — the test name is the
contract, the fixture number is indicative — this is **`h22_false_gutter`**, five pages with a 75 pt
empty band down the middle of every line. Nothing on those pages says whether it is a gutter; the
evidence is between the pages, which is the point R10 §6.5 is making.

Affects: `crates/openconvert/src/pipeline.rs`, `crates/oc-layout/src/continuity.rs`,
`crates/oc-testkit/src/handmade.rs` (h22), thresholds `layout.columns.continuity_break_max` and
`layout.columns.max_column_retries`, test 3.5, PIPELINE §6 step 4.

## 2026-09-13 · Columns are detected before blocks, from run coverage · Phase 3
Context: PIPELINE §6 lists segmentation first and column detection second. Implemented in that order, the
column retry of step 4 cannot do anything: by the time the columns are known, the blocks have already
been built across or within them, and re-running with `k−1` produces the same blocks and the same order.

Decision: detect columns **first**, from the x-projection of *run* boxes — which is what §6 step 2 says
the projection is over ("project glyph coverage onto x"), not block boxes — then split any line that
spans a gutter, then segment, then order. Segmentation is unchanged; what changes is that the hypothesis
it works from is a hypothesis, and can be withdrawn.

That also puts the Phase-2 carry-forward in its proper place. A line spanning a gutter has one box, one
indent and one right gap across both columns; splitting it is `layout`'s job because the split is exactly
as good as the column hypothesis, and under `k = 1` there is no split to make. `words` breaks a *run* at
`text.line_split_gap_em`, which is all the projection needs to see the valley.

Affects: `crates/oc-layout/src/columns.rs` (`detect_columns` now takes run boxes, the page em and a column
cap), `crates/oc-layout/src/blocks.rs` (`LayoutLine` carries its segments), `crates/oc-text/src/lines.rs`,
`crates/openconvert/src/pipeline.rs`, PIPELINE §6, PROGRESS.md carry-forward 2.

## 2026-09-13 · The unwrap factor is 0.45, and page 0 of f01 has four paragraphs, not five · Phase 3
Two small corrections made while implementing paragraph reconstruction, both recorded because the numbers
they change are quoted elsewhere.

**`paragraph.line_unwrap_factor` 0.4 → 0.45.** PIPELINE §7 step 4 is explicit: "The generic Calibre HTML
path uses 0.4 (R10 §6.4); 0.45 is the PDF-path value and is the anchored default here."
IMPLEMENTATION_PLAN Phase 3 detail 4 quotes 0.4, and `thresholds.toml` was written from the plan. The
authority order settles it — PIPELINE outranks IMPLEMENTATION_PLAN — so the value is 0.45 and the
evidence string now says which of Calibre's two defaults it is and why.

**Test 3.6's assertion.** The plan's table says "convention `FirstLineIndent`; 5 paragraphs on page 0".
Page 0 of `f01` carries a heading and three paragraphs, which is four; there is no fifth. Following the
rule that the test *name* is the contract and its assertion is measured, the test asserts four and names
each one. The heading is a paragraph at this stage by construction: `layout` assigns no semantics, and
"Chapter 3" becomes a `Heading` in Phase 4, where `structure` is what decides that.

Also recorded, because it surprised a test before it surprised a book: "short last line" is measured
against **the block's own measure**, so two short lines under each other with nothing above them are one
paragraph, not two. That is the rule working. A paragraph's last line is short *relative to the lines
above it*, and a block whose every line is short has no long line to be short against.

Affects: `thresholds.toml`, `crates/oc-layout/src/paragraphs.rs`, test 3.6, PIPELINE §7 step 4.

## 2026-09-13 · Dehyphenation: what the tiers decide, and what they deliberately do not · Phase 3
Three things worth recording from implementing PIPELINE §7 step 6, all of them about the *shape* of the
answer rather than the code.

**1. `f01`'s `pipeline` is a classifier case, and the deterministic tiers keep the hyphen.**
`f01_prose_single_column.assert.json` asserts that `pipe-` + `line` comes out as `pipeline`. It does not,
yet, and that is the fail-closed rule working exactly as specified: the document never uses the word
`pipeline` anywhere else, so the in-document tier abstains; the English list contains `pipe` and `line`
as independent words, so tier T4's "both halves attested" rule says *keep*; and PIPELINE §7's fail-closed
condition — joined form absent from the in-document dictionary, absent from the lexicon, halves both
attested — is met exactly. This residual is precisely the population R2 §B.7 measures the classifier on,
where a dictionary-only baseline scores 31.7 % keep-recall against the classifier's 85.8 %. The assertion
is an acceptance artefact for Phase 5 and stays as it is; test 3.7 uses `h23`, whose join the *document's
own vocabulary* settles, so that it tests the merge and the join rather than the tier that has not landed.

**2. A fractional budget cannot be measured on a hundred-character document.** The first `h23` carried
one legitimate hyphen in 110 non-whitespace characters — nine parts in a thousand against
`conservation.budget.dehyphenate`'s five — and the stage refused the conversion. Nothing was wrong with
either the removal or the budget: a fraction of a very small number is dominated by its numerator. The
fixture was lengthened to a little over 200 characters, which is the least a document can be and still
have a single hyphen measured against a five-in-a-thousand allowance. Recorded because the same
arithmetic will come back on the first one-page real document, and the answer there is a *floor* on the
budget denominator, which is Phase 6's to decide with the rest of the breach policy.

**3. The in-document lexicon carries its own locale.** Folding is locale-sensitive in one of v1's three
languages, and a lexicon built with Turkish folding and queried with invariant folding looks up keys
nobody wrote. The tag is stored on `DocLexicon` at build time rather than passed at each lookup, so the
two cannot disagree.

Affects: `crates/oc-text/src/dehyphen/`, `crates/oc-layout/src/paragraphs.rs`,
`crates/oc-core/src/stages/paragraphs.rs`, `crates/oc-core/src/ledger_check.rs` (I-5),
`crates/oc-testkit/src/handmade.rs` (h23), tests 3.7, 3.8, 3.10, 3.11, 3.17.

## 2026-09-13 · German: the capital after the hyphen decides it, without a lexicon · Phase 3
Context: plan Phase 3 detail 5 gives German "a dependency-free compound acceptor: try each internal split
point, accept when both halves (allowing `-s-`/`-n-`/`-es-` Fugenlaute) are attested". That needs
somewhere to look words up, and D15's German frequency list does not exist — the sources the plan named
are CC-BY-SA (item 2.9, still open).

Decision: two rules, not one.

1. **An upper-case continuation means the hyphen is real**, and this needs no lexicon at all. German
   capitalises a noun at its first letter and nowhere else, so a word broken across a line always
   continues in lower case: `Fahr-` / `zeug`, never `Fahr-` / `Zeug`. A capital after the break is a
   hyphen the author wrote. This is the orthography rather than a frequency heuristic, and it is what
   decides test 3.9's `Nord-` / `Süd-Achse`.
2. **The compound acceptor** for the lower-case case, written against an attestation *predicate* rather
   than against a list. In v1 the predicate is the document's own vocabulary, which exists today; when
   the German list ships it is one argument rather than a rewrite.

The acceptor's floor of three characters per part is load-bearing: `an`, `ab` and `in` are all German
words, so a two-character floor finds a compound seam in almost every word it is shown.

Also, two smaller things from the same item:

- **`f06_hyphenation_de` forces its two line breaks** with `#linebreak()` instead of letting Typst's
  hyphenation patterns place them. What the fixture has to guarantee is that `Nord-` ends a line and
  `Süd-Achse` begins the next; a fixture whose subject moves when an upstream pattern file is updated is
  a fixture that tests the upstream. The geometry is identical either way.
- **`typst_fixtures_are_reproducible` no longer asserts a fixture count of five.** Its subject is that
  compiling a fixture twice produces the same bytes; a hard-coded count turns "a phase added a fixture"
  into a failure that says nothing about reproducibility. The floor of five stays, so an empty directory
  still fails.

Affects: `crates/oc-text/src/compound_de.rs`, `crates/oc-text/src/dehyphen/tiers.rs`,
`corpus/fixtures/typst/f06_hyphenation_de.typ`, `xtask/src/fixtures.rs`, test 3.9.

## 2026-09-13 · The classifier, trained on the word list's own sources · Phase 3
Context: plan Phase 3 detail 5 and test 3.12 — "a kilobyte-scale logistic/CRF over character features,
trained by `eval/` with its weights committed", gated at keep-hyphen recall ≥ 0.80 on a 2,000-item
holdout.

Decision: train it from the **same twelve CC0 Standard Ebooks the English word-frequency list is built
from**, because that licence question is already settled (D15) and a training set is as much a shipped
artefact as a word list — the weights are derived from it. Keep examples are the corpus's genuinely
hyphenated types split at their own hyphen (`well-known` → `well` / `known`); join examples are ordinary
types split at a seeded interior point. Eight thousand hashed features, FNV-1a, 32 KB of float32 — the
"≈ 30 KB" the plan budgets.

Evidence, from `crates/oc-text/src/dehyphen/model.sources.json`: 39,817 types seen, 21,263 training
examples of which 717 are keeps, 2,000 held out of which 239 are keeps, split by type so no word appears
on both sides. **Holdout keep-recall 0.912, join-recall 0.930, balanced accuracy 0.921** — against R2
§B.7's 85.8 % and 92.38 % for the same kind of model, and against a dictionary-only baseline's 31.7 %.

Two things this changes downstream, both worth naming:

**The split points are approximations.** A real line break falls where a hyphenation pattern allows one;
ours fall at a seeded interior point, because the patterns are not ours to redistribute (VD-b). It costs
the model the finer grain of *where* a typesetter would break, not *whether* a break is a break.

**`f01`'s `pipeline` now has a measured answer, and it is the wrong one.** The classifier scores
`pipe` / `line` at +0.96 — a confident *keep* — so `f01` comes out as `pipe- line` and its committed
assertion is not met. The cause is visible in the data: the training corpus is twelve nineteenth-century
novels, `pipe-line` is a perfectly ordinary spelling in that register, and the modern compound never
appears. This is one of the ~7 % of joins the model gets wrong, it fails in the safe direction — a
visible hyphen rather than a corrupted word — and the fix is a corpus with a modern register, which is
Phase 7's. The assertion stays as it is: it is a correct expectation of a finished system and an honest
record of the gap.

Affects: `eval/src/oc_eval/train/hyphen_clf.py`, `crates/oc-text/src/dehyphen/classifier.rs`,
`crates/oc-text/src/dehyphen/model.bin` + `model.sources.json`, `eval/data/hyphen_holdout.jsonl`,
`thresholds.toml` (`dehyphen.classifier_margin_min`), tests 3.10 and 3.12, Phase 7 corpus.

## 2026-09-13 · The whitespace cover searches one column at a time · Phase 3
Context: the layout dump made the cross-check visible for the first time, and on `f02` it flagged ten of
fifteen blocks — on a fixture whose segmentation is plainly right. A cross-check that fires on a clean
page teaches everyone to ignore it, so it was worth finding out why before blessing a snapshot of it.

Three faults, each found by the one before it:

1. **The rectangle budget was being spent on the wrong page.** `layout.whitespace.max_rectangles` is 40,
   PdfPig's number — measured for finding *column separators* over a whole page. Used for block
   segmentation, the biggest forty rectangles of a two-column page are the gutter, the ragged right
   edges and the empty foot of the shorter column; the bands between paragraphs, the only rectangles
   that separate anything, were never emitted. **The cover now runs once per column**, with its own
   budget and only that column's obstacles. Ten flags fell to three.
2. **A rectangle that separates nothing was still being emitted.** The white wedge left by a paragraph's
   short last line is maximal and large, and it reaches neither pair of the region's opposite edges. A
   separator does: a band crosses the column, a gutter runs down it. Emission is now filtered on that,
   and because the branch and bound narrows a candidate at every pivot, each result is **grown to
   maximality** before the test — a band found inside one branch had been narrowed to that branch's
   width and would have failed a test it deserved to pass.
3. **The two columns were being joined by the grouping, not by the cover.** Two lines at the same height
   in different columns overlap vertically, and neither column's cover contains a rectangle spanning the
   gutter — it cannot, since each searches its own column. They are now separated by the *page's* column
   hypothesis, which is where that fact lives.

**What is left, and why it stays.** One block of `f02` is still flagged: the cover cuts `pages.` off from
the three lines above it. A line box is an *inked* box, so a line with no ascenders is shorter than its
neighbours and the band above it is 5.2 pt against a 4.5 pt floor, where Docstrum — measuring baseline to
baseline — sees nothing unusual. Fixing it properly means giving each line the slug it was set in rather
than the ink it carries, which needs an ascent and a descent this stage does not have. It is left as it
is because the direction is right: over-flagging a confidence signal costs a line in a report, and the
flag changes no segmentation — Docstrum's answer stands either way.

Affects: `crates/oc-layout/src/blocks.rs`, tests 3.1 and 3.15, `layout.whitespace.max_rectangles`.

## 2026-09-13 · A digest may not carry anything derived from geometry · Phase 3
Context: `digest_h22_layout` passed on Windows and failed on Ubuntu in CI, on one field and by one unit:
`min_agreement_iou_milli` 101 against 102.

Cause: the digest was written to be the thing that *can* be asserted across hosts — "counts and totals,
no geometry" — and then two of its fields were computed from geometry. An IoU is a ratio of areas, and
`h22` is set in a non-embedded base-14 face, so its glyph boxes differ between operating systems by a
couple of hundredths of a point (`docs/DECISIONS_LOG.md`, 2026-09-13, and D13.8's contract, which cannot
hold for such a document). Rounding to thousandths does not make a derived quantity stable; it only
moves the boundary it is unstable at.

Decision: `min_agreement_iou_milli` is **removed**. How many blocks were flagged is the *decision* the
cross-check produced and it stays (`low_confidence_blocks`); how nearly each one missed is a measurement,
and it belongs in the dump, where it already is.

`paragraph_candidates` had the same flaw for a different reason and was fixed rather than removed: it
counted lines with `indent_pt > 0.0`, a strict comparison against a float carrying the same hundredths.
It now counts lines indented by at least `paragraph.indent_min_em`, which no rounding difference can
cross and which is also what the word means to the stage that reads it. The numbers it reports changed
in a way worth recording: `h22` went 25 → 0, because under the one-column hypothesis its lines are not
split at the false gutter and every line starts at the same margin — the old 25 *was* the instability,
counted.

Evidence: run 34777494044, job `test (ubuntu-latest)`; `test (macos-latest)` and `test (windows-latest)`
passed, which is what a one-host-in-three failure looks like when the cause is font substitution.

Affects: `crates/openconvert/src/dump_layout.rs`, tests 3.16 and its `h22` companion, D13.8.

## 2026-09-14 — a table cell gutter narrower than a run split

**Measured on `f10`.** Typst's default table inset is 5 pt, so two 10 pt cells sit about
9 pt apart — 0.9 em, under `text.line_split_gap_em` (1.2). `words` therefore keeps the two
cells in one run, and `"layout"` and `"Conserving"` arrive as `"layoutConserving"` with one
box spanning the vertical rule between them.

A `Run` carries a box and its text, not its glyphs' positions, so `structure` cannot split it.
Three options were considered:

1. **Lower `text.line_split_gap_em`.** Rejected: the number is about word spacing in a line,
   and lowering it would split justified prose whose word gaps stretch, which is a regression
   in every book to fix a case in some tables.
2. **Split runs at vertical rules in `layout`**, the way `split_lines_at_gutters` splits them
   at page gutters. This is the right long-term answer and it is real work: the rules would
   have to reach `layout`, which today receives none.
3. **Detect the straddle and take the image fallback.** Taken. `extract_tables` checks whether
   any run inside a table's region crosses a column rule and, if so, emits the table as the
   image plus its text with `W_TABLE_AS_IMAGE` and `reason = "a run crosses a column rule"`.
   It loses the grid and keeps every character, which is the direction PIPELINE §8.7
   prescribes for everything it cannot read cleanly, and R2 §B.9's scope for v1: detect tables
   well enough not to destroy them.

`f10`'s table is set with `inset: 8pt` so that its gutters are 1.6 em and the *gridded* path
is the one test 4.13 exercises. A tightly set ruled table is a known gap; option 2 closes it,
and the corpus in Phase 7 is what says how often it matters.

## 2026-09-14 — Phase 4 fixture numbering

The plan's Phase 4 Files list names `f06_footnotes`, `f07_novel_structure` and
`f08_lists_and_table`, and `h15`–`h20` for the hand-made ones. All of those numbers were spent
in Phases 2 and 3. Phase 4 therefore uses **`f08`, `f09`, `f10`** and **`h24`–`h29`**, and
`f07` is `f07_verse_and_quote` — the fixture the plan listed under Phase 3 and Phase 3
deliberately did not write, because no Phase 3 test named it and verse is PIPELINE §8.6.
`PROGRESS.md` predicted `h24` for test 4.10; it went to test 4.8 instead, the hand-made
fixtures being assigned in test order.

## 2026-09-14 — a heading is its own block, and the size barrier that makes it one

Docstrum reads geometry and nothing else. On `f09` the gap between a 14 pt heading and the
10 pt line under it is 14.1 pt against a body leading of 13.1 pt — 7 % — which no distance
multiplier can catch without splitting every paragraph in the book. The size difference is
29 %.

PIPELINE §6 defines a block as "one paragraph, one heading", so `layout.block.size_barrier_ratio`
is that definition being enforced rather than a heuristic added on top of it. Three properties
were load-bearing:

- It applies to **both** segmenters. The barrier is a fact about the page, not a property of
  either algorithm; applied to Docstrum alone it would make the two disagree at every heading
  and flag the whole book low-confidence.
- It reads the line's **dominant** size — the size of its longest segment — which keeps a drop
  cap, the largest glyph on a body line, inside its line rather than in a block of its own.
- `paragraphs` needed the same barrier. Typst, like most book designers, does not indent the
  first line after a heading, so with the heading in a block of its own the indent convention
  had nothing to break on and reconstruction merged them straight back.

Cost: `f01`'s digest goes from 4 blocks to 5 and `f02`'s from 15 to 16 — the headings — and
`f02`'s low-confidence count falls from 2 to 1, the barrier being a place the two segmenters
now agree.

## 2026-09-14 — superscripts that are neither raised nor small

PIPELINE §4 step 4 reads a superscript as an origin moved off the baseline **and** a smaller
size. Typst, through Libertinus, sets footnote markers with an OpenType `sups` glyph: drawn on
the baseline at the body size, with only the outline raised. Measured on `f08`: the marker's
origin y is identical to the letter before it, `size_pt` is identical, and the ink box sits
3.6 pt clear of the baseline at 10 pt.

The old rule read that as ordinary text, so the marker was swallowed into the middle of a body
run — `"a reference1 in the"` — where nothing downstream could find it. A second rule was added:
a glyph whose whole inked box sits at least `footnote.superscript_rise_ratio × size_pt` clear
of the baseline is a superscript.

It is restricted to the digits and the symbol cycle, and that restriction is the only thing
that separates the two cases: a `sups` digit and a right single quotation mark are
geometrically identical. The costs are not symmetric — a mis-flagged apostrophe splits `don't`
into three runs and emits `don<sup>'</sup>t`, a visible corruption, while a missed marker loses
a link the bijection check then reports.

## 2026-09-14 — the list marker stays in the item's text

`structure` is Conserving and `Reason` is a closed set of fifteen variants (IR_SKETCH). None of
them is "a list marker", so a stage that stripped `1.` from an item would be removing text it
cannot account for, and the conservation check would fail — correctly.

So the marker stays in `Para.text` and `ListItem.marker` records it. `epub` knows that `<ol>`
draws its own numbers and is the stage that may elide exactly that prefix; deciding how it
declares that is Phase 5's.

## 2026-09-14 — `Decision.subject` is optional, because two of the first decisions are not about a block

IR_SKETCH gives `Decision { subject: BlockId, … }`. The first two decisions the pipeline
actually records are the document class and the preset, and both are choices about the *book*:
there is no block to name. Filling the field with a block id chosen for the purpose would make
a document-level decision indistinguishable from a decision about whichever block that id
belongs to, which is exactly the confusion the decisions log exists to prevent.

So `subject: Option<BlockId>`. IR_SKETCH says documents may elaborate but must not contradict;
this is the smallest elaboration that keeps the record honest, and a per-block decision still
carries its block.

## 2026-09-14 — a page break before a heading, and where it is recorded

`book_structure` moves a section's opening heading out of the content list and into
`Section.heading`. A page-break walk that looked only at content would therefore attach the
break for a page that begins with a chapter title to the first *paragraph* of that chapter, and
"go to page 57" would land past the title of the chapter that starts on page 57.

The break is recorded as the first item of the section's content list, and `epub` lifts a
leading run of `Content::PageBreak` above the heading. The alternative — a second place for
`epub` to look, keyed on `before_block == heading.id` — puts the same rule in two places and
makes the flow no longer the authority on order.

## 2026-09-14 — `furniture` recovers no folio from a book that changes numbering system

`f09` paginates its front matter `i, ii` and its body `1, 2, 3`. Digit masking puts the three
arabic folios in one group and the two roman ones in groups of one, so the largest folio group
covers 3 of 5 pages: a repetition ratio of 0.6, inside the grey zone `[0.30, 0.70)`, where the
detector abstains. The folios stay in the flow as one-character paragraphs and every
`PageRef.label` is `None` — which also removes the arabic-1 reset that PIPELINE §9 step 1 calls
a hard boundary signal.

Not fixed here: it is a `furniture` rule, the fix is a change to how a folio group is scoped
(per numbering system, or per pagination run), and choosing between those wants the Phase 7
corpus rather than one fixture. Phase 5's label test therefore runs against `f01`, where the
folios are recovered, and asserts what the `document` stage owns: that a label `furniture`
found reaches the page break that opens its page.

## 2026-09-14 — `noteref` returns `Phrasing`, and `anchor_cannot_nest` tests nesting

The plan's builder sketch has `noteref` return "Phrasing sans anchors", and test 5.2's
assertion column reads `noteref(...).noteref(...)` fails to compile. Implemented that way, a
paragraph with two footnotes would be unrepresentable — which is not a rare shape, it is most
of academic prose — and, worse, the natural emitter becomes unwritable: the emitter folds a
paragraph's spans into one phrasing element in a loop, and a loop cannot change the type of its
accumulator halfway through.

So the type that forbids `<a>` inside `<a>` is the *content* of an anchor, not its successor:
`link` hands its closure an `El<NoAnchor>`, and `El<NoAnchor>` has neither `link` nor
`noteref`. `noteref` itself takes the marker as plain text and has no closure at all, so
nothing can be placed inside one by construction.

`anchor_cannot_nest` keeps its name and tests what the name says — an anchor inside an anchor,
both spellings — rather than two anchors in sequence, which is legal XHTML and legal in books.

## 2026-09-14 — an XML-illegal character is refused, not dropped

XML 1.0 has no spelling for a C0 control other than tab, line feed and carriage return:
`&#1;` is as ill-formed as the raw byte, so escaping cannot rescue one. `epub` is Conserving
with an empty ledger and `Reason` has no variant that covers "a character XML could not
carry", so dropping one would be removing text the stage cannot account for.

The emitter therefore refuses, and that is the honest signal rather than a cop-out: a control
character in the body flow means a page that decoded to garbage took the text path, and
PIPELINE §2 routes those pages — `broken-text` — to OCR or to a page image precisely so that
they do not. "There is no fallback path. An emitter failure is a bug" (PIPELINE §10).

Open, and for the ADR rather than for this phase: if a real book turns out to reach `epub`
with a control character in it, the fix is a `Reason` for it in `ingest`, not a silent drop in
the serialiser.

## 2026-09-14 — an `<img>` with no alt text cannot be built

`Figure.alt` is empty when nothing could be derived, and `alt=""` is EPUB's way of marking an
image *decorative*. Tier 1 requires every `<img>` to carry at least one non-space character
(the ACC-001 class, D6), so emitting `alt=""` for every unlabelled figure would be both a
validation failure and a false claim about the book's illustrations.

`ImgRef::new` therefore returns `None` on empty alt text, which moves the decision to the one
place that has the context to make it and makes "an `<img>` with no alt" unrepresentable in the
same way an illegal nesting is.

## 2026-09-14 — a list keeps the markers the book printed, and says so in CSS

`structure` leaves `1.` inside the item's text, because `Reason` has no variant for a list
marker (entry of 2026-09-14 above). `epub` is Conserving too — D13.4 names "chapter splitting,
XHTML serialization" among the Conserving operations and I-3 gives them empty ledgers — so
`epub` may not elide the prefix either. The plan's Phase 4 note left *how it declares that* to
this phase, on the assumption the elision would happen here; under D13.4 it cannot.

So the list is emitted as `<ol class="list-printed-markers">` with the printed marker still in
the item text, and `style.css` sets `list-style-type: none` on that class. The semantics a
screen reader needs are on the `<ol>`; the markers a sighted reader sees are the ones the book
printed; and not one character moved.

## 2026-09-14 — `RunId` is page-local, whatever IR_SKETCH calls it

IR_SKETCH describes `RunId` as a "per-document run index (stable within one extraction)".
`oc_text::words::assemble_runs` numbers runs from zero on every page, so run 7 exists once per
page of the book. Keyed on the id alone, a map from run to note marker silently loses one
marker per collision: on `f08` the spans came out `[2, 1, 2]` against note refs `[0, 1, 2]` —
the first footnote's reference overwritten by the third page's run of the same index.

Every map from a run is therefore keyed on `(page, RunId)`, and `crate::build::NoteRefRuns`
names that pair once so the next one cannot get it wrong. Renumbering runs document-wide would
be the other fix; it is an `ir_version` question and a change to a Phase 2 stage, so it is not
this phase's.

## 2026-09-14 — spans are built where the runs still are

Phase 4 set `Para.spans` to a single plain span, and IR_SKETCH gives `structure` the job of
filling them. Phase 5 needs them filled for a reason that is not cosmetic: `NoteRef` names the
*run* that printed a marker, and unless that run becomes a `Span` carrying the note's id there
is nothing for `epub` to turn into `<a epub:type="noteref">` — the emitted book would have
footnote bodies and nothing pointing at them, which is the `RSC-007` bijection failure test
5.10 exists to catch.

So `build::para_of` now splits the paragraph at every style change and every note marker. The
hard constraint is `spans_text(&spans) == text`: `Para` carries both and the conservation check
reads one of them, so a span list that said anything else would make I-3 pass on a document
that does not exist. `a_paragraphs_spans_are_its_text_split` asserts it on six fixtures, and a
`debug_assert` in `para_of` asserts it on every paragraph of every test run.

A drop cap joined from its own block is prepended as a span rather than folded into the text
for the same reason: rebuilding `spans` from the joined string would throw away every style and
every note reference the paragraph's runs carried.

## 2026-09-14 — `dcterms:conformsTo` is not emitted

IMPLEMENTATION_PLAN Phase 5 detail 2 lists `dcterms:conformsTo` "matching the required string
pattern" among the required package metadata. PIPELINE §9.6 says the opposite in as many words:
"Never auto-claim WCAG conformance — the tool cannot guarantee it from PDF source."

In EPUB Accessibility 1.1 that property *is* the conformance claim; there is no version of it
that means "some accessibility work was done". The authority order puts PIPELINE above the plan,
and a false conformance claim is a worse defect than a missing optional property — an
institutional buyer filtering on it would get a book that does not meet what it says it meets.

So it is absent, and `schema:accessibilitySummary` says plainly what was done and that no
conformance is claimed. EPUBCheck 5.3.0 reports neither an error nor a warning for its absence.

## 2026-09-14 — `fuzz_xhtml_emitter_roundtrip` is a property test, not a fuzz target

The plan names `cargo-fuzz` for row 5.19. A fuzz target is not a test: it has no pass condition,
it runs until someone stops it, and CI cannot hold it to "green" — which is exactly what §0.3
item 1 requires of every named row.

It is a `proptest` with a generator weighted towards the characters that break serialisers, and
the property is two-sided: the emitter must either produce a document that parses, or refuse.
Refusing is a legitimate answer for a character XML 1.0 cannot carry, and a test that demanded
output would be demanding the wrong thing. `PROPTEST_CASES=4096` is the nightly tier (§0.5).

## 2026-09-14 — a continuation fragment carries `aria-label`, not `aria-labelledby`

Phase 5 detail 6 says a mid-chapter split's continuation fragments are "plain `<section>`
continuations with the same `aria-labelledby`". `aria-labelledby` is an IDREF and may only
reference an element in the *same document*; a continuation's heading is in the previous file, so
the attribute would point at nothing.

The first fragment keeps `aria-labelledby` pointing at its own heading, which is better than a
repeated string because the two cannot drift apart. The continuations carry `aria-label` with the
heading's text, which is self-contained and valid. Both keep the fragments reading as one chapter,
which is what the detail is for.

## 2026-09-14 — what EPUBCheck found that seventy tests did not

Two defects, on the first run of the Tier-2 gate over the ten fixtures:

- Image `src` was written package-root-relative — `images/i0001.jpg` — from a document living in
  `text/`, so every figure resolved to `text/images/…` and was missing. `RSC-007`, twice on `f10`.
  Tier 1 checked that *fragments* resolved and never that *resources* did; it checks both now.
- A document that yielded no text produced an empty spine, an empty nav `<ol>` and an empty
  `navMap`: three `RSC-005`s on `f03`, and not a publication at all. PIPELINE §10 already said
  what to do — "pages that failed to yield text … become an image inside a `<figure>` … rather
  than being dropped silently" — and the `document` stage now does it.

Both are recorded here because they are the argument for D6's Tier 2 being a hard gate rather
than a nice-to-have: seventy tests written against this emitter, including a Tier-1 validator
whose whole job is to find this class of defect, and neither of these surfaced until an outside
implementation read the output.

## 2026-09-18 — retention is a flag in Phase 6, and Appendix D wants a number it cannot have

I-7 holds on all ten fixtures on the first run, measured over the archive. The retention ratio it
carries does not clear `validate.min_char_retention`:

```
f01 0.9687   f02 0.9888   f03 —(no source text)   f04 1.0000   f05 1.0000
f06 1.0000   f07 1.0000   f08 0.9677   f09 0.9704   f10 0.9727
```

Retention is `|C(EPUB)| / |C_0|`, and `C_0` is the pdfium text layer — a running head and a folio
are part of it. Four fixtures remove furniture and land at 0.968–0.973, all of it ledgered and
all of it inside the 0.04 furniture budget. So **`validate.min_char_retention = 0.98` and
`conservation.budget.furniture = 0.04` are jointly unsatisfiable** for any book whose furniture is
near its budget: 4 % of `|C_0|` legitimately removed puts retention at 0.96.

Resolved for this phase by taking the two sources at their word. R6 §11 says "**flag** character
retention < 98 %" and `IMPLEMENTATION_PLAN` Phase 6 row 6.3 says `retention_below_threshold_warns`
— a `Warning`, carrying the measured value, which is what `oc_validate::structural` emits. The
furniture budget is D13.4's and wins on authority (CLAUDE.md §1) over a threshold introduced in
`IMPLEMENTATION_PLAN` §1.5.

**Left open for Phase 7 and Appendix D**, where the corpus can answer it: Appendix D's v1.0 item
"character retention ≥ `validate.min_char_retention` on every non-scanned stratum" reads as a hard
gate, and as written no book with a running head on every page can pass it. Either the floor moves
to `1 − global_non_ocr_removal` (0.92), or the metric becomes retention *of text no reason
accounts for* — which is I-7 itself, and would make the second gate redundant. Phase 7 has the
strata to choose; guessing now would put a number in `thresholds.toml` with no evidence behind it.

A book whose source carried no text — `f03`, and every scanned book until Phase 13 — has no
retention ratio at all, and `retention_warnings` returns nothing for it rather than 0.0. Appendix D
says "non-scanned stratum" for the same reason.

## 2026-09-18 — the structural validator, and one dependency edge the crate map does not list

`oc-validate` now depends on `oc-text` (and through it on `oc-core`). ARCHITECTURE §3.1's table
lists only `oc-model` and `oc-epub` against this crate, while §7.2 puts duplicate detection inside
the structural validator and PIPELINE §11 says that detection is the Gopher/MassiveText repetition
family — which lives in `oc-text` and reads its bounds from `oc-core`. The alternative to the edge
is a second implementation of nine statistics, one measured over source pages and one over output
text, free to drift. Neither `oc-text` nor `oc-core` reaches `oc-ai` or `oc-net`, so the three rules
§3.1 calls load-bearing (`oc-pdf` and `oc-ai` never meet; `oc-ai` has no network crate; `oc-epub`
depends only on `oc-model`) are untouched, and there is no cycle: `oc-core` depends on `oc-model`
alone, as every other crate that reads thresholds already assumes.

**Three fields of `StructuralReport` are projections of the Tier-1 report, not second checks.**
`image_parity`, `note_bijection` and `hrefs_resolve` are things Tier 1 already measures over the
archive; PIPELINE §11 lists them under Tier 1 and ARCHITECTURE §7.2 lists them again under the
structural validator, because the structural report is what the user is shown. Two implementations
of one bijection would be two chances to get it wrong.

**The Gopher n-gram statistics cannot be a gate over output text, and `f09` is why.** Its
`top_3gram` share is **0.8008**, against a `quality.top_3gram_frac` bound of 0.18 — and the book is
correct. The cause is its printed contents page: the dotted leader is hundreds of repetitions of
`. . .`, and the statistic is character-weighted, so three quarters of that page's characters are
one 3-gram. So the structural report *records* the nine statistics and warns on none of them; what
it warns on is `DuplicateStats`, over emitted **blocks**, at `validate.dup_block_frac = 0.02`.

That split is the substantive one. `quality.dup_para_frac = 0.30` is datatrove's, and it routes a
*source* page to review — a threshold about how repetitive human writing gets. A block emitted
twice in the *output* is a pipeline bug: the motivating case, *AI Engineering*, duplicated 21 526
characters because a table detector claimed text that stayed in the flow, and at ~100 blocks per
50 pages that is nowhere near 0.30. Two orders of magnitude tighter is the resolution the finding
needs. It is not zero because a kept running head, a repeated `Notes` heading and a boilerplate
copyright line are legitimately identical blocks.

**The h1-count range is scoped by page count.** "A plausible h1 count for a book is 2–60" says
nothing about a two-page fixture, where "at least two chapters" is arithmetic rather than evidence.
Below `validate.h1_count_min_pages = 20` the answer is `None`, and every fixture is below it — so
the check is written and tested and is first exercised for real on the Phase 7 corpus.

## 2026-09-18 — VD-f is deferred to Phase 15, and why that is not a dodge

The verification-debt table names Phase 6 as where **VD-f** blocks: "validation-pack JRE licence, per
vendor — that a Temurin (or other OpenJDK-derived) minimal `jlink` image is redistributable under
GPLv2 + Classpath Exception has to be read from that vendor's own licence text."

Phase 6 ships no validation pack. The plan's own scope line says so: "**Not in this phase:** … the
in-app validation pack (Phase 15)." Nothing this phase produced links, bundles or downloads a JRE —
`oc_validate::epubcheck` and `oc_validate::ace` both *invoke* an executable the environment supplies,
which is why they are behind cargo features and run in CI rather than on the conversion path. The
question VD-f asks is about **redistribution**, and there is nothing to redistribute until the pack
exists.

So the row is **deferred to Phase 15**, with that reason, which is what the Phase 0 table's own rule
asks for ("closed, or explicitly deferred … with the reason recorded in `docs/DECISIONS_LOG.md`"). It
is recorded here rather than left to the reading of "or whenever the validation pack ships", because a
row that is open at the phase it names and unexplained is indistinguishable from one that was
forgotten.

The sibling row **VD-e** stays open and unchanged: it blocks the optional dictionary pack, which is
post-v1, and D15 already routes around it for core data.

## 2026-09-19 — what the first CI run of Phase 6 found that six phases of local testing did not

Phase 6 merged to `main` with 455 tests green on Windows and every local gate clean. The push run
failed four jobs and the nightly failed one. Every failure was real; none was a flake. They are
recorded together because the pattern matters more than any one of them: **each was invisible to a
machine that only ever ran the tests on one operating system with one toolchain.**

### 1. The container's bytes were not the same on Windows as on Linux and macOS

`epub_is_byte_identical_across_os` failed on its first real run, and `golden_epub_bytes_f01` failed
on macOS. Ubuntu and macOS agreed with each other; Windows did not. The entry list was identical and
the total was identical — 3812 bytes both ways — which is what made it hard to guess and easy to
prove: **a fixed-width field was varying.**

It was the "version made by" host byte. `zip`'s `SimpleFileOptions::default()` fills it from the
*building* platform — `System::Dos` on Windows, `System::Unix` everywhere else — and the crate's own
test suite documents exactly that. `zip.rs`'s module comment already claimed "a timestamp, the
host-system byte in the version-made-by field, and the unix-permission extra fields. All three are
pinned here". Two were. The sentence was written from intent rather than from the API, and nothing
on one machine could contradict it.

Fixed by `.system(zip::System::Unix)`. The proof is arithmetic rather than argument: with the field
pinned, this Windows machine produces `b18e9a8d…` for `f01` — the byte-for-byte value macOS and
Ubuntu had produced all along, and the value the committed snapshot has been corrected to.

**A first guess that was wrong, recorded because the elimination is the evidence.** The obvious
suspect was `core.autocrlf`: the Typst fixture sources were checked out CRLF on Windows and LF
elsewhere, the PDF's SHA-256 mints `dc:identifier`, and a `urn:uuid:` is fixed-width — the same shape
of symptom. It is not the cause: Typst normalises line endings itself, and recompiling every fixture
from LF sources leaves the PDF digests unchanged. `.gitattributes` gained `eol=lf` anyway, so a
Windows working tree matches what CI checks out, but it is hygiene and it fixed nothing.

### 2. Ace found three real accessibility defects on its first run

The `ace-a11y` job failed, and the gate was right to fail:

- **`epub-pagesource`, *serious*.** A book that publishes page numbers must say where they came
  from, or a citation of "p. 42" names a page in nothing in particular. Fixed by emitting
  `pageBreakSource` — valued as `urn:sha256:<digest of the source PDF>`, because that is the only
  handle this converter honestly has: a PDF carries no ISBN, and the filename is the user's business
  and does not belong in a file they may hand to someone else.
- **`metadata-accessmodesufficient`, moderate.** `schema:accessModeSufficient` was emitted under
  `if has_alt`, which is the condition *inverted*: it claimed textual sufficiency only for books that
  had images and withheld it from books that were nothing but text. It is unconditional now — the
  typed builder cannot emit an image without alt text, so `textual` is always sufficient.
- **`epub-type-has-matching-role`, moderate, on every content document of every fixture.**
  `<section epub:type="chapter">` carried no `role="doc-chapter"`. `EpubType::role` now carries the
  DPUB-ARIA mapping, taken from the table Ace checks against rather than from memory, and returns
  `None` for the four types — `frontmatter`, `bodymatter`, `backmatter`, `footnotes` — that have no
  role, because inventing one would assert a semantic the specification does not define.

EPUBCheck still reports 0 errors on all ten fixtures after all three.

### 3. The Ace runner would have hidden the metadata half of its own gate

Two bugs in the runner, and the second is the instructive one.

Its error said `EOF while parsing a value at line 1 column 0` and threw away Ace's stderr, so the
first CI cycle bought no information at all. `AceError::NoReport` now carries Ace's exit status,
stdout and stderr. When a subprocess fails, what it said is the entire diagnosis.

And `parse` read the metadata from `data.metadata`, **a key Ace does not write**. Every required
property came back missing on every book. The unit tests passed because their fixture JSON was
composed from Ace's documentation by the same hand that composed the parser — so they tested the
misreading against itself. The fixture is now a real `report.json`, trimmed, and the parser reads
`a11y-metadata.present`, which is Ace's own answer. `empty` is deliberately not folded into
`present`: a property declared blank satisfies a checker that only looks for the element and tells a
reader nothing.

A parser for someone else's format has to be tested against their output, not against one's reading
of their documentation. This module's own doc comment had said the reader "would silently report zero
violations if the schema moved" — and it was already doing the metadata half of exactly that.

### 4. Phase 6 pushed the Ubuntu runners off the end of their disk

`test (ubuntu-latest)` and `no-network` both died with `collect2: fatal error: ld terminated with
signal 7 [Bus error], core dumped`, which is what a full filesystem looks like from inside the
linker. Not a flake and not GitHub's fault: every integration test statically links the whole
pipeline including the PDFium binding, `openconvert` alone contributes twenty such executables, and
Phase 6 added eight more.

`[profile.dev] debug = "line-tables-only"` takes the workspace's test executables from **5.7 GB to
488 MB** — a twelvefold cut — and keeps the file and line of every frame, which is all a panic
backtrace in CI needs to name the assertion that failed. The two Linux jobs additionally reclaim the
preinstalled toolchains they do not use and print `df -h` either side, so the next person does not
have to infer the disk state from a signal 7.

### What this says about the Definition of Done

Phase 6's Definition of Done named two rows it could not verify on one machine and declined to count
them as passes. Both of them failed. That is the entry working as intended, and it is the argument
for writing a partial row as partial rather than as "basically done": the two rows that were honest
about being unverified are precisely the two that were broken.

## 2026-09-19 — the second CI run, and two jobs that had never run at all

The five defects above are fixed and their jobs are green: `test` on all three operating systems,
`no-network`, `epub_is_byte_identical_across_os`, and — on its first real run —
`dom-checks`. `webkit-dom` passed nightly too.

Two jobs then failed that had never executed before. Both were skipped in the first run because
`needs: test` had failed, and both die in the same step for the same reason:

    tar: This does not look like a tar archive
    Error: tar failed unpacking .../epubcheck-5.3.0.zip

**`xtask fetch-epubcheck` unpacked a zip with `tar`.** EPUBCheck's release asset is a `.zip`; every
other asset the workspace fetches is a `.tgz`. `tar -xf` opens a zip on Windows, where `tar` is
libarchive, and GNU tar refuses it. The code has been in the tree since Phase 5, has been run on
this Windows machine many times, and could not fail until a Linux runner reached it — which needed
the `test` job to pass first.

Fixed by unpacking in process with the `zip` crate the workspace already has, with the same
zip-slip refusal `fetch-epubcheck-corpus` already used for the same reason. The refusal is fatal
here rather than skipped: a release asset with a traversal entry in it is not an archive with one
bad file, it is an archive to stop trusting. Verified by moving the vendored copy aside, fetching
from scratch, and diffing the two trees — `diff -rq` reports nothing, so the in-process extraction
reproduces what `tar` produced byte for byte.

`Command::new("tar")` now appears nowhere in `xtask`, and the only asset that is not a tarball is
the one that no longer goes through it.

**Ace's Electron needed two things from the runner.** The improved error paid for itself on its
first use and named them exactly: `chrome-sandbox` must be root-owned and setuid — `npm install
--global` unpacks it as the runner user, and Electron aborts rather than run unsandboxed — and
Electron on Linux needs a display even for a window it never shows. The job configures the sandbox
rather than setting `ELECTRON_DISABLE_SANDBOX`, because turning off a security boundary to run a
checker over our own fixtures is the wrong trade even on a throwaway runner, and runs the test under
Xvfb.

### The pattern across all seven

Not one of the seven was a flake, and not one could have been found on this machine. Four were
platform divergence that a single-OS run cannot see (a zip host byte, a zip unpacked by a tool that
differs per platform, a setuid bit, a display). Two were a gate finding real defects the moment it
first ran — which is what a gate is for. One was resource exhaustion that only appears at CI's
scale.

The Definition of Done's habit of naming a row it cannot verify, rather than rounding it up, is what
made this tractable: the rows that failed were the rows already marked as unverified.

## 2026-09-20 · `xtask fixtures --keep-structtree` tagged every fixture, not ~12.6 % of them · Phase 7
Context: Phase 7's `oc-eval corpus lint` implements IMPLEMENTATION_PLAN §1.8's rule that the synthetic
bucket's `tagged` share stays within ±5 points of `corpus.tagged_share_target` (0.126, D18 / R1 §A.10).
It fired on the manifest as committed: 6 tagged of 16 synthetic entries, 0.375. Reading the generator
showed the number is worse than it looks — `run()` emitted a `__tagged` variant for *every* `.typ`
source, so a full regeneration would have produced 10 tagged of 20, exactly 0.5. The committed manifest
was 6 only because the task had last been run when six sources existed.
Decision: `TAGGED_FIXTURES` names the fixtures that get a tagged variant, and it names two:
`f01_prose_single_column` (English) and `f04_german_prose` (German). Two of ten sources is 2/12 = 0.167,
inside 0.126 ± 0.05, and it is the widest coverage the share allows — one fixture would also fit at
0.091, and would exercise the tagged path in one script only. The four surplus entries
(`f02`, `f03`, `f05`, `f06` tagged variants) are deleted from `corpus/manifest.json`; the generator no
longer produces them, so they do not return. `f01_prose_single_column__tagged` is the only tagged
fixture any test names (`crates/oc-pdf/src/meta.rs`), and it is kept.
A new threshold `corpus.tagged_share_tolerance = 0.05` carries §1.8's ±5 points, so the Rust gate and
the Python lint read one number instead of two that agree today.
Evidence: `xtask::fixtures::the_tagged_bucket_is_the_share_the_real_world_has`,
`xtask::fixtures::every_named_tagged_fixture_is_a_fixture_that_exists`,
`eval/tests/test_corpus_lint.py::test_a_synthetic_bucket_that_kept_its_struct_trees_is_a_finding`.
Affects: D18, IMPLEMENTATION_PLAN §1.8, `xtask/src/fixtures.rs`, `thresholds.toml`, `corpus/manifest.json`.

## 2026-09-20 · A harvested licence is verified by the harvester, and says so · Phase 7
Context: TEST_CORPUS §7.1 requires every corpus entry to carry `license.verified_by` and
`verified_date`. The Phase 7 harvest admits ~100 real documents, and a human did not read ~100 licence
statements.
Decision: `license.verified_by` records `oc-eval corpus harvest` — the agent that actually read the
field, from the source's own machine-readable rights metadata (OAPEN `dc.rights.uri`, Internet Archive
`licenseurl`, arXiv OAI `<license>`, NTRS `copyright.determinationType`, and the `<link rel="license">`
on a DergiPark article page). Writing `maintainer` there would be a claim nobody made. Every licence
is normalised through `stratify.license_from_url` and checked against §7.1's allowlist, so a
non-commercial or no-derivatives record is named and rejected rather than quietly skipped; §7.4's
annual re-verification remains a human job and is what would change this field.
Evidence: `eval/tests/test_corpus_harvest.py::test_a_candidate_whose_licence_is_not_acceptable_is_rejected_by_name`,
`eval/tests/test_corpus_sources.py` (one refusal test per source).
Affects: TEST_CORPUS §7.1 §7.4, `corpus/manifest.json`, `eval/src/oc_eval/corpus/harvest.py`.

## 2026-09-20 · Two mutation recipes are not reproducible, and the catalogue says so · Phase 7
Context: PHASE 7 row 7.6 asks that each mutation recipe applied to its parent reproduce the committed
mutant byte-for-byte. Running `cargo xtask mutations` twice showed the three encrypted mutants
rewritten with different bytes each time — 902 bytes on one run and 903 on the next. AES-128 draws a
fresh initialisation vector for every string and stream, so encrypting the same file twice cannot give
the same file twice. That is the cipher working, not a defect.
Decision: the catalogue carries `Reproducibility` per recipe. `Deterministic` recipes are asserted
byte-for-byte against the committed mutant, which is row 7.6 as written. `Randomised` recipes are
asserted on the property that makes byte-equality impossible — two applications to the same parent
must differ — so a recipe that quietly became deterministic, which would mean a broken cipher, fails
the same test. `xtask mutations` no longer rewrites an existing randomised mutant: regenerating it
replaced a regression artefact with noise and put an unreviewable diff in front of whoever ran the
task. `--regenerate` is deliberately absent; deleting the file is the way to ask for a new one.
Evidence: `xtask::mutations::tests::every_mutation_recipe_replays`.
Affects: IMPLEMENTATION_PLAN PHASE 7 row 7.6, `xtask/src/mutations.rs`.

## 2026-09-20 · The Phase 7 mutation catalogue is Rust, not the Python of §1.7 · Phase 7
Context: IMPLEMENTATION_PLAN §1.7 lists the mutation recipes as Python modules under
`eval/src/oc_eval/mutate/` and PHASE 7 §4 calls for `pikepdf`/qpdf-QDF recipes. `oc-testkit::mutate`
already held four of them in Rust, with the reason written in its module docstring in Phase 1: a
metamorphic test has to apply the mutation and compare in one process, and a Python step in the middle
would make `cargo nextest` — the gate — depend on an interpreter, a virtualenv and a wheel.
Decision: the five recipes the failure taxonomy was missing (`strip_structtree`, `double_draw`,
`ocr_sandwich`, `jitter_spacing`, `damage_xref`) are Rust, beside the four that were already there.
Row 7.6's test walks one catalogue rather than two, the fast tier keeps them, and `xtask mutations`
can apply the same recipes to a corpus document when the eval harness wants one. The plan's intent —
a recipe committed as a reviewable diff — is met by the recipe being committed source with the
mutant's bytes committed beside it, which is what a reviewer reads either way.
Not done: **Type 3 re-encoding**, the sixth item in PHASE 7 §4's list. Re-encoding an embedded font as
Type 3 while preserving its outlines needs a glyph-outline extractor that `lopdf` does not have, and a
Type 3 font whose CharProcs draw rectangles would change what the page looks like rather than only how
it is encoded. It belongs with the handmade fixtures — a small PDF authored as Type 3 with a
`/ToUnicode` map — and is recorded in `PROGRESS.md` as a gap rather than faked here.
Evidence: `xtask::mutations::tests::every_mutation_does_what_its_effect_promises`, which holds each
recipe to a declared `Effect` through PDFium; flipping one declared effect turns it red.
Affects: IMPLEMENTATION_PLAN §1.7 and PHASE 7 §4, `crates/oc-testkit/src/mutate.rs`,
`xtask/src/mutations.rs`.

## 2026-09-20 · The benchmarks live in `openconvert`, not `oc-core` · Phase 7
Context: IMPLEMENTATION_PLAN PHASE 7's file list puts the criterion benches at
`crates/oc-core/benches/{stages.rs,end_to_end.rs}`.
Decision: they are `crates/openconvert/benches/{stages.rs,end_to_end.rs}`. `convert` lives in
`openconvert`, and `oc-core` is below it in the dependency graph — a bench in `oc-core` that drove the
pipeline would need `oc-core` to depend on `openconvert`, which is a cycle. The plan's list predates
the stage driver landing in `openconvert` (Phase 5 item 5.8, "the pipeline moved into the library").
Evidence: `crates/oc-core/Cargo.toml` has no path dependency on `openconvert` and cannot acquire one.
Affects: IMPLEMENTATION_PLAN PHASE 7 file list.

## 2026-09-20 · The wall-clock budget assertions are behind a cargo feature · Phase 7
Context: PHASE 7 rows 7.11 and 7.12 are `bench-gate` rows. Run on every PR they would assert wall
clock on a shared CI runner that is simultaneously building three other jobs.
Decision: `crates/openconvert/tests/perf_budget.rs` splits in two. The arithmetic — that row 7.12's
five stage budgets sum to `perf.seconds_per_page_max`, that each is a positive share of it, and that
the reference book is the 300 pages D13.11 states the budget for — needs no clock and runs on every
PR, because that is where the mistake that actually happens gets caught: a stage quietly given more
room than the whole has to give. The timed assertions are behind the `bench` feature, which the
nightly `bench` job turns on. A feature rather than `#[ignore]`, for the reason CLAUDE.md gives and
the `epubcheck` and `ace` features already follow: a feature is something a job turns on and a
developer can too, while an ignored test is one nobody ever runs again.
Measured on the maintainer's machine, unoptimised `test` profile: **0.0217 s/page over 300 pages**
against a 0.5 budget, and the gate was confirmed to fail when the budget was lowered below it.
Machine L's number will differ; this is a floor on the headroom, not the reference measurement.
Evidence: `openconvert::perf_budget::the_stage_budgets_sum_to_the_end_to_end_budget`,
`openconvert::perf_budget::timed::bench_end_to_end_within_budget` (with `--features bench`).
Affects: D13.11, IMPLEMENTATION_PLAN PHASE 7 rows 7.11, 7.12, `thresholds.toml`.

## 2026-09-20 · Phase 7.5 inserted: the corpus converted 0 of 14, and no phase owned the fix · Phase 7
Context: Phase 7's first real corpus run. A random sample of 14 holdout documents converted **none**:
11 refused by I-1, all of them in `structure`, with losses from 48 to 203 205 characters; 2 exceeded a
180 s timeout, both Internet Archive scans; 1 was a harness error on my side (a `/dev/null` output
path on Windows) and not a pipeline failure. Appendix D's first correctness item — "I-1 … I-7 hold on
100 % of the corpus" — is a v1.0 release gate and does not hold. Phases 8–15 are AI abstraction, local
model, AI decisions, BYO providers, desktop UI, OCR, security and packaging: **no phase's job was to
fix what the corpus found.** The plan assumed the deterministic path was correct once Phase 6's own
tests passed, and Phase 7 was only to measure it. The measurement disagreed.
Decision: PHASE 7.5 — Conservation defect closure — is inserted between Phases 7 and 8, and Phase 8
does not start before it. The ordering is not tidiness: Phase 10 calibrates each AI decision by
McNemar comparison against the deterministic baseline, and a baseline that loses a fifth of a book is
not something a comparison against it means anything about.
The phase's governing rule, at the maintainer's direction: **a fix may not be derived from a single
document.** The unit of work is a defect class keyed on `(stage, direction, signature)`, admitted only
at ≥ 3 documents across ≥ 2 producer strata — the same reasoning as D18's `ours(*)` cap and
`calibration.min_gold_instances_per_task`, that a conclusion drawn from too few observations is not a
conclusion. Every class carries a written *How else could this arise?* answered **before** any code
changes, because that is the step that turns a symptom into a mechanism and it is what produces the
sibling fixtures. A class closes by making the fault unrepresentable or checked, never by
special-casing the shape it was found in, and it leaves behind an invariant test rather than only a
corpus file.
Two failure modes are named in the phase and guarded by its tests: fixing documents instead of classes
(the reward for which arrives immediately — the book converts), and making a refusal disappear by
widening a `conservation.budget.*` or inventing a `Reason` meaning "text we could not account for",
which would convert a refusal into a silent loss and be worse than shipping nothing. No item in Phase
7.5 may change `conservation.budget.*` or add a `Reason`.
Evidence: the 14-document sample above, reproduced from `corpus/downloads`; a minimal reproducer
already isolated — `oapen-20-500-12657-115632.pdf` page 42 alone loses 644 characters in `structure`,
found by hand-written binary search over page prefixes, which is why the phase's first item is a
diagnostic.
Affects: IMPLEMENTATION_PLAN (new PHASE 7.5, Appendix C's phase map), PROGRESS.md, Appendix D.

## 2026-09-20 · The line between the LLM and the deterministic pipeline, as a test · Phase 7.5
Context: D13.5 and D13.6 already draw the boundary — an LLM edit must be `Conserving`, "labels/levels/
roles/CSS classes only", and "label authority is not deletion authority". It is stated across two
decisions and a call-shape paragraph, which is enough to follow and not enough to settle an argument.
Decision: the boundary gets one question, stated in `docs/LLM_BOUNDARY.md` and derived from the
existing decisions rather than invented: **if this question is answered wrongly, does the book lose or
gain a character?** If yes, the answer is deterministic, permanently, and no amount of model quality
changes it. If no, the question is about a *name* and may be escalated.
Deterministic by that test: which characters exist (extraction, dehyphenation, ligature expansion),
which are removed and under what `Reason` (furniture, overdraw, OCR-layer duplicates), which block
owns a character (table cell assignment, note body assembly, caption binding), and reading order.
LLM-eligible, and exactly D13.6's four tasks: metadata fields, heading role per style cluster,
front/body/back boundaries, verse-vs-quote for an ambiguous indented block — all of them names for
characters whose identity is already fixed.
What makes it a boundary rather than a preference: an `oc-ai` property test asserts that applying any
edit the grammar permits leaves `C(D)` unchanged, and a CI lint holds `LLM_BOUNDARY.md` and `oc-ai`'s
task enum in agreement in both directions.
Affects: D13.5, D13.6, new `docs/LLM_BOUNDARY.md`, PHASE 7.5 rows 7.5.8 and 7.5.9.

## 2026-09-20 · Phase 7.5 gains the reading corpus: the product is for novels · Phase 7.5
Context: Phase 7's corpus is open-access monographs, journal articles, government technical reports
and library scans. That population was chosen for **licence clearability**, which is the right way to
build a redistributable corpus and the wrong way to build a representative one. OpenConvert converts
PDFs into reflowable EPUBs for people to read; the document it exists for is a novel — continuous
prose, chapters, running heads, notes, a contents page, and almost nothing else. `--preset novel` is a
first-class preset in §2.1 and **no novel has ever been through the pipeline**. A converter measured
only on monographs is tuned on two-column layouts, dense tables, affiliations and reference lists —
the features a novel does not have — and untested on the one that matters.
Decision: Phase 7.5 assembles a **reading corpus** before it fixes anything: 40–50 documents per
language across English, German and Turkish, all prose people read end to end, under the same licence
discipline and the same `oc-eval corpus harvest` admission path as Phase 7. It is a separate stratum
axis rather than a replacement — Phase 7's corpus is what proves a two-column paper with a reference
list survives, and D18's per-stratum reporting means the two are never averaged.
**Reading-corpus entries are not marked `holdout`.** That is deliberate and is what makes the phase
legal: these are the documents the pipeline is developed against, fitting on them is the point, and
the frozen ≥ 100-document holdout stays frozen and unread so it can still report at the end.
Three properties of this population matter to the defect work. It is **mostly scans**, so
`ABBYY-scanner` stops being 16 documents and becomes the largest stratum — the one with an OCR text
layer and a hyphenation habit of its own. It is **long**, 200–600 pages of uninterrupted body text,
which is where the cross-page passes are first exercised at scale: furniture repetition, paragraphs
continuing over a page break, footnotes carried to the next page — and the one defect already isolated
is a footnote that spans a page boundary. And **German and Turkish become a third each** rather than a
ten-document slice, so R10 §6.3's dotted and dotless i gets a population.
The per-language target is 40–50 rather than ten because of the phase's own admission rule: a class
must be observable on ≥ 3 documents across ≥ 2 strata and ≥ 2 languages before anyone may touch it,
and a ten-document language cannot supply that for anything but the most common fault.
Evidence: the 14-document sample (0 conversions, 11 I-1 refusals all in `structure`); `--preset novel`
in IMPLEMENTATION_PLAN §2.1 with no novel in `corpus/manifest.json`.
Affects: IMPLEMENTATION_PLAN PHASE 7.5 (goal, implementation item 1, rows 7.5.0a-c, A7.5.0),
TEST_CORPUS §3 and §7.6 (a second population beside the holdout), PROGRESS.md.

## 2026-09-20 · Public-domain literature needs its own admission basis · Phase 7.5
Context: the first candidate list for the English reading corpus — seventeen public-domain novels on
the Internet Archive — was run through Phase 7's admission rule. It admitted **two**. The other
fifteen are Austen, Melville, Dickens, Twain, the Brontës, Verne, Hugo, Dumas, Stoker, Wells and
Carroll: unambiguously public domain, and rejected.
Cause: `stratify.license_from_url` admits a document by reading a machine-readable licence — OAPEN's
`dc.rights.uri`, an Internet Archive `licenseurl`, an arXiv `<license>` element. That is the correct
and sufficient basis for CC-BY material, which is what Phase 7 sourced. It cannot express the basis on
which a novel from 1813 is free: its author died in 1817. That is a fact about the **work**, and it
appears in no field of the **item**. Most public-domain scans on the Internet Archive carry no
`licenseurl` at all.
Decision: `PD-old-work` gets a second admission path rather than a loosened first one. The manifest
entry records `author` and `author_death_year`, `license.evidence` names both, and admission is
`author_death_year + 70 < current year`. Loosening `is_acceptable_license` to accept a missing licence
was rejected: it would admit anything an uploader forgot to label, which is the opposite of what
TEST_CORPUS §7.4 asks for.
Two checks come with it, because a public-domain work does not imply a redistributable scan.
**Lending collections**: an Internet Archive item in `inlibrary`, `lending` or `printdisabled` is
offered under controlled digital lending and is refused whatever the work's age. **Translations**: a
translation is a work of its own, so an entry naming a translator is dated by the *translator*. Both
were written against the real list — one of the seventeen was Robert Fitzgerald's 1961 Odyssey, in a
lending collection, and it is the single entry the corrected rule rejects.
A process note worth keeping: the subagent that produced the list reported `"license": "PD-old-work"`
and a Creative Commons public-domain-mark URL for **every** entry, including items whose metadata
carries neither. Those fields were not read from the source; they were filled in. A subagent's
`verified: true` is a claim, and the corpus admission path is what turns a claim into evidence — which
is the argument for having the admission path at all.
Evidence: 2 of 17 admitted under the Phase 7 rule, 16 of 17 under the corrected one, the one rejection
being the in-copyright translation.
Affects: IMPLEMENTATION_PLAN PHASE 7.5 implementation item 1 and rows 7.5.0x-z, TEST_CORPUS §7.1,
`eval/src/oc_eval/corpus/stratify.py`.

## 2026-09-20 · The reading corpus's three languages are not equally available · Phase 7.5
Context: three subagents produced candidate lists for the reading corpus, one per language. Each list
was then run through the corrected `PD-old-work` admission — author death year + 70, not in a lending
collection, item holds a PDF, a translation dated by its translator.
Results: **English 16 of 17 admitted. German 23 of 24. Turkish 4 of 10.**
The Turkish rejections are not a sourcing failure, they are a fact about the language, and it is worth
writing down because it will not change by searching harder:

| Work | Author | Died | Public domain in |
|---|---|---|---|
| Çalıkuşu | Reşat Nuri Güntekin | 1956 | **2027** — misses by one year |
| Yeni Turan | Halide Edib Adıvar | 1964 | 2035 |
| Yorgun Savaşçı | Kemal Tahir | 1973 | 2044 |
| İstanbul'un Bir Gecesi (İthaki, 2018) | Suat Derviş | 1972 | 2043 |
| Başakların Sesi (1968) | Müjgan Cunbur | 2005 | 2076 |

Turkish copyright runs life + 70, and the alphabet reform was **1928**. A Latin-script Turkish novel is
therefore public domain only if its author both wrote after 1928 and died before 1956 — a window of
twenty-eight years. Everything older is in Ottoman Arabic script, which is a different extraction
problem and not what `--preset novel` is for; everything newer is in copyright. The four admitted
entries sit on the wrong side of that: Namık Kemal (d. 1888) and Tevfik Fikret (d. 1915) are
Ottoman-script or verse, and the two Halit Ziya Uşaklıgil works (d. 1945) straddle the reform.
Decision: the Turkish slice is sourced from the window rather than from a general search. The authors
who fall inside it and are widely read are **Sabahattin Ali (d. 1948)** and **Sait Faik Abasıyanık
(d. 1954)**; both wrote in Latin script, both are in every Turkish school curriculum, and both are
public domain now. Reşat Nuri Güntekin enters on 1 January 2027 and the manifest should carry the
entry with its date so it can be admitted then rather than rediscovered.
If the window cannot supply forty documents — and it may not — the Turkish slice is **reported short
with its reason** rather than padded with Ottoman-script scans that answer a different question. A
slice that reaches its count by changing what it contains has not met the target; TEST_CORPUS §7.6
already says this about the holdout and it applies here.
A second process note, sharper than the first: the Turkish subagent labelled **every** entry
`PD-old-work` or `CC0`, including Kemal Tahir's *Yorgun Savaşçı* — which is the same file
`docs/TEST_CORPUS.md` §7.5a already lists in `example_pdfs/` with the note "**Assume in copyright**
(Kemal Tahir, 1965) … Local use only." The project's own documentation contradicted the agent's
licence claim, and only the admission check caught it. Redistributing that file would have been an
infringement committed by a corpus built to be redistributable.
Evidence: `corpus/reading/candidates_{en,de,tr}.json` against the death-year admission.
Affects: IMPLEMENTATION_PLAN PHASE 7.5 item 1 and A7.5.0, TEST_CORPUS §7.6.

## 2026-09-20 · Defect class `structure/lost/claim-without-emission`, admitted and worked · Phase 7.5
Context: `openconvert diff-stage structure` (row 7.5.1) was pointed at the corpus. Of the first 22
documents, **7 across 2 producer strata** report blocks claimed by a structure that never emitted
them, which meets this phase's admission rule (≥ 3 documents, ≥ 2 strata). The diagnostic names the
claimant `(unnamed)` when *no* container even covers the block, which is the signature: a claim was
recorded for a block that nothing built.

```
arxiv-2305-09065  pdfTeX   23 894 lost  list:322 note:1 table:64   22 776 characters under `list (unnamed)`
arxiv-2309-01261  pdfTeX   16 703 lost  list:38  note:1 table:95
arxiv-2303-09565  unknown   2 855 lost  list:17  table:10
arxiv-2201-05139  pdfTeX      458 lost  list:12
arxiv-2109-08745  pdfTeX      451 lost  list:4   table:62
arxiv-1902-00488  pdfTeX      200 lost  list:4   table:9
ia-152017-20170505 unknown      29 lost  table:17
```

Mechanism: **the claim is computed from a predicate over the input, not from what the claimant
built.** Two independent instances, both of that one shape.

- `lists::detect_lists` does `consumed_lines.extend(start..=last_marked)` — every line of the marked
  run, marked or continuation. `build_level` then emits `lines.get(marker.line)` — the marked line
  **only**. Its own comment says "its marked line and every unmarked line beneath it before the next
  marker" and the code takes one line. Every continuation line inside a list run is therefore claimed
  and never emitted.
- `tables::extract_tables` does `outcome.consumed.extend(blocks inside region.bbox)` — every block
  geometrically inside the region, whether or not `build_table` put its text in a cell.

*How else could this arise?* — answered before the fix, because the answer is what makes the fix
architectural rather than two patches:

1. **Any claimant whose claim is a predicate over its input.** Both instances above. A geometric
   test and a line range are guesses about what the builder will do, made before it does it.
2. **A builder that abstains after the claim is recorded.** `build_list` returns `None` below
   `list.min_siblings`; today the claim happens after, so this is safe *by accident of statement
   order* rather than by construction.
3. **A builder that emits less than it claimed because of a cap.** `build_level` stops at
   `list.max_depth` and returns `nested: None`; markers deeper than that are inside the claimed run
   and appear nowhere.
4. **A container built and then never referenced from the flow** — the reverse face of the same
   fault. A figure whose image was dropped as an ornament keeps its bound caption claimed; a table
   whose `first_block_of` returns `None` is never pushed into the flow. `emitted_text` counts both,
   so the stage balances and the book is short. `ia-2003-nov` shows it: 179 characters lost against
   the declared output and **559** against the reachable one.
5. **The same shape in three later stages.** `document` moves blocks into chapters, `epub` moves a
   document into XHTML files, `repair` edits a `Document` in place. Each keeps bookkeeping about
   what it has handled, and none of them is checked either.

Decision: **a claim may not be asserted; it is derived from what the claimant emitted.** The
builders return the units they actually placed, `consumed` is exactly that set, and `structure`
asserts before returning that every claim's text is contained in the text its claimant emitted *and*
that the claimant is reachable from the flow. Stated in `oc-core` over the IR rather than inside
`oc-structure`, so points 5's three stages get it without being changed.

Rejected: fixing `build_level` to include continuation lines and stopping there. It closes the
instance and leaves the class — the table instance, the `max_depth` instance and the three later
stages would all still be able to happen, and the obvious symptom would be gone.

Evidence: `openconvert diff-stage structure corpus/downloads/*.pdf`; tests
`a_claimed_block_must_be_accounted_for_by_its_claimant`,
`every_container_the_conservation_check_counts_is_reachable_from_the_flow`.
Affects: `oc-structure::{lists,tables,stage,claims}`, `oc-core::conservation_diff`, PHASE 7.5 items 4-5.

## 2026-09-20 · The fixture suite could not have found this, and that is a finding · Phase 7.5
Context: `every_container_the_conservation_check_counts_is_reachable_from_the_flow` passes on **all
ten Typst fixtures** and fails on real documents. So does the claim/emission gap: `structure` is
conserving on every fixture and loses 23 894 characters on an arXiv paper.
Decision: recorded, not fixed. The fixtures are authored, so their lists are well formed, their
tables are ruled, and every container they build is referenced. That is what a fixture is for and it
is also its limit: **a synthetic corpus cannot exhibit the defects of a producer nobody wrote.**
This is the argument for the reading corpus (item 7.5.0) stated as a measurement rather than as an
expectation, and it is why the phase's closure criterion is "against both corpora" rather than
"against the fast subset".
Evidence: `cargo nextest run -p openconvert --test diff_stage` green; the sweep above.
Affects: PHASE 7.5 item 1, TEST_CORPUS §7, Appendix D.

## 2026-09-20 · Class `structure/lost/claim-without-emission` closed, and what it cost · Phase 7.5
Context: the fix decided in the entry above, measured on the eleven real documents the class was
admitted from. `diff-stage structure`, reachable output, before and after.

```
document              before lost/dup    after lost/dup
arxiv-2309-01261        17 769 /   1      13 370 /   0
arxiv-2305-09065        24 072 /   2      23 228 /   0
arxiv-2303-09565         2 855 /   1         620 /  18
arxiv-2201-05139           458 /  17         152 /   9
arxiv-2109-08745           451 /  14           0 /  77
arxiv-1902-00488           200 /   9          48 /  66
ia-152017-20170505          29 /   0           3 /   0
arxiv-2305-14528            20 / 424           0 / 424
ia-2003-nov                559 /   0         559 /   0
TOTAL                   46 413 / 544      37 980 / 681
```

**Every `(unnamed)` claimant is gone** — 62 blocks across the set were claimed by a structure that
did not exist, and none is now. 18 % of the loss went with it, and no document lost more than
before.

Two findings came out of doing it, and both changed the fix:

1. **The first attempt made two documents worse.** Deriving the table's claim from the runs it took
   consumed blocks that only *partly* overlap the region, so their outside text left the flow and
   nothing put it back. Corrected to the rule list detection already states: a block is claimed only
   when **every** one of its non-empty units went to the claimant.
2. **That correction traded loss for duplication** — 544 → 2 015 characters emitted twice — because
   a straddling block then stayed in the flow while its inside runs were still in the cells. The
   answer is the statement the class is really about: **the unit of taking and the unit of claiming
   must be the same unit.** The table reads text at *run* granularity and claims it at *block*
   granularity, and every block straddling the edge falls in that gap — one way it is lost, the
   other way it is duplicated. A straddling block is now left alone entirely.

Open, and deliberately not closed here: **the same unit mismatch on the list side**, which is why
duplication still rose from 544 to 681. `detect_lists` takes at *line* granularity and claims at
*block* granularity, and a block mixing introductory prose with a list item now has its list lines
in the item and its prose in the flow. That is a distinct class with its own evidence, and closing
it inside this one would be fitting a fix to the documents that happened to be on the bench —
precisely what this phase's rule forbids. Recorded as `structure/appeared/claim-unit-mismatch-list`.

Evidence: `cargo nextest run --workspace` 480 green; the table above.
Affects: `oc-structure::{lists,tables,stage}`, PHASE 7.5 items 4-5.

## 2026-09-20 · The "timeout class" did not exist; silence invented it · Phase 7.5
Context: PROGRESS recorded a timeout class — 13 corpus documents producing no output within
180 s across three producer strata — and set it as the next work item. It was wrong.

Nine of the thirteen fail in **200-740 ms**. One is a `furniture` I-4 budget refusal. Two are
genuinely slow (32 s and 67 s) and complete; they were killed by the sweep's own timeout while
the sweep was being stopped. A 9-page, 0.9 MB document was among the "timeouts" and takes a
quarter of a second to fail.

Cause: `EventSink::emit` returns early when `--progress json` is off, and `fatal` went through
it. `inspect`, `validate`, `dump-stage` and `diff-stage` all report errors only through the
sink, so they exited non-zero having printed nothing. The sweep captured an empty file and a
non-zero exit and I labelled it `TIMEOUT_OR_FAIL` — a guess that then travelled into PROGRESS
as a fact, with a stratum count attached to make it look measured.

Decision: a `fatal` is not telemetry a caller may decline. It is the program's answer to what
it was asked to do, and it now always reaches the user — as an NDJSON event when that channel
is on, as one prose line on stderr when it is not, which is the rule `--locale` already follows
for warnings. Ordinary events stay silent; the exemption is for the fatal alone.

Two lessons kept, because both will recur:
- **An empty output and a non-zero exit are not evidence of a hang.** The harness must record
  the *reason*, and where there is no reason the correct entry is "no reason was reported",
  not a guess at one.
- **A label written in a throwaway script becomes a fact.** `|| echo "TIMEOUT_OR_FAIL $name"`
  was shorthand in a shell loop and it ended up as a defect class with a stratum breakdown in
  the project's live state file. The inventory (item 7.5.3) must be generated from the
  diagnostic's own output, never from an exit code.

Evidence: `a_fatal_reaches_the_user_without_progress_json`; timings above.
Affects: `oc-core::events`, PHASE 7.5 item 3 and item 6, D13.2 §2.3.

## 2026-09-20 · Class `text/substituted/nfc-singleton`, recorded and BLOCKED · Phase 7.5
Context: with fatals visible, the nine sub-second failures resolve to two mechanisms. Seven of
them are one: `openconvert diff-stage text` names it as U+2126 OHM SIGN leaving and U+03A9
GREEK CAPITAL LETTER OMEGA appearing, 1 to 58 times per document.

`N`'s third component is NFC, U+2126 has a canonical singleton decomposition to U+03A9, and
D13.4's `Reason` enum is closed with no variant for canonical composition — while ARCHITECTURE
§5.2 defines `C(D)` over Unicode *scalars*, which U+2126 and U+03A9 are two of. The law
therefore refuses a document in which nothing was lost, and the refusal cannot be ledgered away
because there is nothing honest to ledger it as.

Decision: **none taken.** This changes the definition of `C(D)`, which CLAUDE.md §1 reserves to
`DECISIONS.md`. Three options, their costs, and a recommendation (state `C(·)` over canonically
composed scalars, so NFC is invisible to the law by construction) are written under PROGRESS
`## Blocked`. `STATUS: BLOCKED`.

Also recorded: the class is **not admitted** under this phase's own rule — seven documents but
one producer stratum. Even with a ruling it waits on the full-corpus inventory, because a
mechanism seen only in pdfTeX output might be a pdfTeX habit.

Evidence: `openconvert diff-stage text corpus/downloads/arxiv-22*.pdf`.
Affects: D13.4, ARCHITECTURE §5.2, `oc-text::normalize`, `oc-model::ledger::c_of`.

## 2026-09-20 · RULING: `C(·)` is taken after canonical composition · Phase 7.5
Context: the question raised under PROGRESS `## Blocked` earlier today — `N` includes NFC,
ARCHITECTURE §5.2 defined `C(D)` over Unicode scalars, D13.4's `Reason` enum is closed with no
variant for canonical composition, and I-1 is checked after the stage that applies `N`. Four
statements, jointly unsatisfiable, and seven corpus documents refused with nothing lost.

Decision: **option B.** `c_of` composes before counting. ARCHITECTURE §5.2 now reads "after
canonical composition (NFC)" and D13.4 carries the amendment. The `Reason` enum stays closed at
fifteen; NFKC stays forbidden.

Why this one rather than a sixteenth `Reason`: it makes the fault **unrepresentable** instead of
merely recordable. No stage can break I-1 by composing, at any point, because both sides of
every comparison are composed by the same function — which is this phase's own standard for a
fix, applied to the law itself. It cannot mask a real loss: NFC is a bijection on the text it
composes, so `c_of("Ω resistance").difference(c_of("resistance"))` is still one omega.

**A consequence the ruling did not anticipate, found by implementing it.** `glyph_chars` built
its histogram by adding glyphs one at a time, bypassing `c_of`. Canonical composition is a
property of a *sequence* — a base and the combining mark after it — so composing one side and
not the other would have moved the defect one function along instead of closing it. Both sides
now go through `c_of`, and so do `LedgerDelta::side` and `reason_side`. The definition of `C`
lives in exactly one function, which is the property that makes the invariant hold.

Measured: four of the seven documents now pass `text` and three of those **conserve perfectly
at `structure` as well** (lost 0, appeared 0).

Evidence: `c_of_is_invariant_under_canonical_composition`,
`c_of_does_not_fold_compatibility_equivalents`,
`c_of_still_sees_a_character_that_actually_vanished`;
`openconvert diff-stage structure corpus/downloads/arxiv-2209-10024.pdf`.
Affects: D13.4, ARCHITECTURE §5.2, `oc-model::ledger::c_of`, `openconvert::pipeline::glyph_chars`.

## 2026-09-20 · Class `structure/panic/spans-text-disagreement`, recorded and NOT admitted · Phase 7.5
Context: with the NFC ruling in, three of the seven documents get past `text` and reach a
**panic** in `oc_structure::build::para_of`:

```
assertion `left == right` failed: the spans are the paragraph's text split, never a different text
  left: "5 2 0 2 n a J 2 ] G"   right: "5 2 0 2  n a J  2 ] G"
```

`para_of` builds `text` by trimming each line and joining with a single space; `spans_of` does
not trim, so an untrimmed line contributes a double space. The difference is whitespace only, so
no character is at risk — but it is a `debug_assert_eq!`, which means **every debug and CI run
on such a document panics**, and D13.4 wants the conservation checks running in exactly those
builds.

Decision: **recorded, not fixed.** Three documents, all `pdfTeX`, and the text is arXiv's
vertical datestamp read bottom-to-top ("G[ 2 Jan 2025"). One producer stratum, and the phase's
admission rule is ≥ 3 documents across ≥ 2 strata — which exists for precisely this shape, a
mechanism that may be one producer's habit rather than a general fault. The full-corpus
inventory decides it.

Evidence: `openconvert diff-stage structure` on arxiv-2304-14883, arxiv-2406-09769,
arxiv-2210-07996.
Affects: `oc-structure::build::para_of`, PHASE 7.5 item 3.

## 2026-09-20 · The NFC ruling is general, and two bypasses that would have made it not · Phase 7.5
Context: the ruling was raised by one character — U+2126 OHM SIGN — and the question was
asked whether the fix follows the character or the mechanism.

Measured. **NFC rewrites 1 120 code points**: 1 002 CJK, 34 Hebrew, 23 Greek, 17 Tibetan, 13
Musical, 8 Devanagari, 6 Gurmukhi, 3 Bengali, 2 Oriya, plus the Angstrom and Kelvin signs
beside the ohm. 85 of the 1 120 are *composition exclusions*, where one scalar becomes several
— so they break I-1 with **unequal** counts and would not even have matched the signature the
first seven documents were found by. And all of that is before the far larger population of
**sequences**: every accented letter a producer chose to draw as a base plus a combining mark,
which is most of German, Turkish, French and Vietnamese in some encodings.

Greek carries the ohm sign's exact shape: U+1F71 GREEK SMALL LETTER ALPHA WITH OXIA is a
singleton that rewrites to U+03AC. A fix reading "if the character is U+2126" would have left
it, and left 1 119 others.

The fix does follow the mechanism — `c_of` composes, so every canonical equivalence folds by
construction — but the test did not prove it. Three examples were replaced with
`c_of_is_invariant_under_canonical_equivalence_across_unicode`, which sweeps **every scalar in
Unicode** and requires a character, its canonical decomposition and its canonical composition
to have one `C`. Mutation-tested: reverting `c_of` to `text.chars()` fails it at **U+00C0**,
the second character it checks, a thousand code points before the ohm sign.

`c_of_folds_the_encodings_the_three_v1_languages_arrive_in` covers Turkish `İ ş ğ`, German
`ä ö`, and Greek in both encodings, and asserts the two things that must **not** fold: `ı` is
not `i` and `İ` is not `I`, because folding either would be a case fold and D13.4 forbids that
outright (R10 §6.3).

**Two bypasses found by auditing every histogram construction site**, and this is the part the
question was worth asking for:

- `oc_pdf::pdfium::doc` built `C_raw` with `c_raw.add(ch)`, one glyph at a time. Composition is
  a property of a sequence, so a base and its combining mark would never have composed there —
  leaving `C_raw` in a different normal form from every other histogram in the pipeline.
  Nothing divides by it today; `report.json` prints it and the retention ratio is one change
  away from using it. Now `c_of(&raw_text)`, guarded by
  `c_raw_is_c_of_the_extracted_text_and_not_a_second_count`.
- `openconvert::pipeline::glyph_chars` had the same shape and was fixed with the ruling itself.

`oc_validate::structural` (I-7) and `LedgerDelta::side`/`reason_side` were checked and already
route through `c_of`. The definition of `C` now lives in exactly one function, and that — not
the ohm sign — is what makes the invariant hold.

Evidence: `c_of_is_invariant_under_canonical_equivalence_across_unicode` (mutation-tested),
`c_of_folds_the_encodings_the_three_v1_languages_arrive_in`,
`c_raw_is_c_of_the_extracted_text_and_not_a_second_count`; 488 workspace tests green.
Affects: `oc-model::ledger::c_of`, `oc-pdf::pdfium::doc`, `openconvert::pipeline`, D13.4.

## 2026-09-20 · Correction: `C(·)` is decomposed, not composed — NFC was the wrong canonical form · Phase 7.5
Context: the ruling earlier today folded canonical equivalence into `c_of` with **NFC**. That
closed the ohm-sign class and opened a subtler one, which the corpus inventory found and no
test did.

Composition depends on **adjacency**. `u` followed by U+0308 composes to `ü` only when the two
are next to each other, and a stage cuts its text where it likes: the glyph stream is one
sequence in *draw* order, the runs are many in *reading* order, the blocks are many again. So a
composing law gives two different answers for one book, and the difference reads as a loss.

Measured, twice, each time one stage later:

```
arxiv-2201-05139, oapen-...-115756, oapen-...-115799   text:   "2 characters left and 4 appeared"
oapen-...-116098                                       text:   "1 character left and 2 appeared"
oapen-...-115756 (after the first correction)          layout: "1 character left and 2 appeared"
oapen-...-116098  diff-stage text:  LEFT U+00FC 'ü'    APPEARED U+0075 'u' + U+0308
```

The last line is the whole story: an umlaut drawn as **one glyph** and assembled into **two
runs that are not adjacent**. Composed on the input side, uncomposed on the output side, and
nothing was lost.

The first correction — `c_of_parts`, which joins the pieces before composing — was necessary
and not sufficient. It made both sides compose over the whole document, which fixed the runs
that *were* adjacent and broke nothing, but draw order is not reading order, so the input side
still composed pairs the output side could not.

Decision: **`C(·)` is taken after canonical *de*composition (NFD).** Decomposition expands each
character independently and the canonical reordering that follows cannot change a *multiset*,
which is what `C` is. So `C` of a text is the same whatever pieces it arrives in — the law is
granularity-independent **by construction** rather than by every stage remembering to cut in
the same place. Verified over every cut position:

```
                       NFD    NFC
cafe + combining acute True   False
u-umlaut decomposed    True   False
I-dot decomposed       True   False
```

It folds exactly what NFC folded — U+2126/U+03A9, ü/u+◌̈, İ/I+◌̇, U+1F71/U+03AC, ş/s+◌̧ — because
two canonically equivalent strings have the same decomposition by definition. And it still
distinguishes what must stay distinct: `ı` is not `i`, `İ` is not `I`, `ﬁ` is not `fi`, `²` is
not `2`.

`c_of_parts` is kept. It is no longer load-bearing for correctness, but every conservation
comparison in the pipeline now goes through one of two functions, and that is the property
worth having.

**What this cost, honestly.** The first ruling was implemented, committed, pushed, and tested
over all of Unicode — and the Unicode sweep passed, because it tests one character at a time
and the fault only appears across a cut. `c_of_does_not_depend_on_where_the_text_was_cut` is
the test that would have caught it, and it exists now. A corpus inventory caught what a
1.1-million-case test did not, which is the argument for running the corpus stated as a
measurement.

Snapshot moved: `structural__retention_per_fixture`. Only the three fixtures with accented
characters, counts only — f04 594→610, f05 569→612, f06 394→399 — and **retention stays
1.0000** on all three, because both sides moved together. The English fixtures are unchanged.

Evidence: `c_of_does_not_depend_on_where_the_text_was_cut`,
`c_of_is_invariant_under_canonical_equivalence_across_unicode`,
`c_of_folds_the_encodings_the_three_v1_languages_arrive_in`; 489 workspace tests green;
all four documents now pass `text`.
Affects: D13.4 amendment, ARCHITECTURE §5.2, `oc-model::ledger::{c_of, c_of_parts}`,
`openconvert::pipeline` (seven comparison sites).

## 2026-09-20 · PROGRESS.md lost 515 lines to a substring match, and how · Phase 7.5
Context: the file that is supposed to let a fresh session resume was silently truncated from
1 094 lines to 579, losing the Phases list, the whole Phase 7.5 write-up, the Notes and the
Phase 7 section. Nobody noticed for three commits.

Cause: a scripted edit anchored on `s.index("## Blocked")`. The string `## Blocked` occurs
**twice** — once as the section heading near the end, and once at line 17 inside the file's own
instructions: *"Write the question under `## Blocked` and stop."* `index` returns the first,
so the replacement consumed everything between line 17 and the end anchor.

This is the second time today the same shape has bitten. The first was
`|| echo "TIMEOUT_OR_FAIL $name"` in a shell loop, whose guess became a defect class with a
stratum breakdown in this file. Both are the same error: **a convenient anchor that is not a
unique one, trusted without checking.**

Decision, and it is a working rule rather than a code change: a scripted edit to a document
anchors on a string that is unique in that document — `"\n## Blocked"` and not `"## Blocked"` —
and the edit is followed by a check that the result still has the shape it should. `wc -l` and
a heading list take one command and would have caught this immediately.

Restored from `811388a`, with the three intended edits since then reapplied: the Blocked
section cleared, `CURRENT_ITEM` moved to the inventory, and the timeout class rewritten with
what was measured rather than what was assumed.
Affects: PROGRESS.md, and the way this session edits documents.

## 2026-09-22 · Defect class `structure/lost/orphaned-claimant`, admitted — 73 % of all measured loss · Phase 7.5
Context: the 95-document inventory, re-read by claimant rather than by document, says who took
the lost text:

```
list        38 docs  5 strata   1 492 414 chars
table       54 docs  5 strata     304 004
(nothing)   64 docs  6 strata     140 849
note        38 docs  6 strata      64 507
caption     31 docs  5 strata      38 907
```

`diff-stage` now names **orphaned claimants** — structures that took blocks out of the flow and
were never placed in it themselves. On the three largest losses, one per stratum, the loss *is*
the orphaned lists:

```
InDesign  oapen-...-115755   lost 217 873   orphaned list text 226 346   9 lists
Word      oapen-...-115632   lost 257 991   orphaned list text 256 856   3 lists
pdfTeX    arxiv-2305-09065   lost  23 228   orphaned list text  23 230   1 list, 451 blocks
```

Mechanism, measured rather than inferred: **13 of 13 orphaned lists have a first block that is
not itself claimed.** A list enters the flow at exactly one point — when the loop reaches the
block holding its first item *and that block is claimed*. Membership is decided per **line**;
claiming is decided per **block**, all or nothing. So when the first item shares a layout block
with the sentence introducing it — "consider the following:" and then "1." on the next line — the
block is only partly the list's, it is not claimed, the trigger never fires, and every block the
list *did* claim is dropped with nowhere to go. The comment above the trigger says "a list is
emitted at the first block it consumed"; the code emits it at the first item's block, and those
are different blocks exactly when this happens. Third time in this phase a comment and its code
have disagreed at the site of a defect.

*How else could this arise?* — answered before the fix:

1. **Any structure whose emission is triggered by a predicate other than its own claims.** Lists
   (the first item's block must be claimed); tables (`first_block_of`, a y-range test that
   ignores columns and can match no block at all).
2. **A claimant whose trigger block is held by another claimant.** 124 contested blocks on the
   InDesign document alone — list + caption, list + table, note + note + note + note.
3. **A container whose own reference is dropped.** A figure whose image is dropped as an ornament
   takes its bound caption with it; a note whose marker is never linked takes its body.
4. **A structure truncated by a cap** — `list.max_depth` — after its lines were claimed.
5. **The same shape one stage later.** `document` moves sections into chapters and `epub` moves
   chapters into files; both keep "handled" bookkeeping and neither is checked for this.

Decision: **a structure is emitted where its first claimed unit occurs, and the unit of claiming
is the unit of taking.** For lists the unit is the line. A block whose lines are all taken is
skipped, as before; a block whose lines are *partly* taken is split — its untaken lines before
the list stay a paragraph, the list is emitted, its untaken lines after follow. Every line is in
exactly one place, and a list with even one taken line is reached by construction, because the
loop visits every block. There is no longer a trigger that can disagree with the claims.

The checked half: `StructureOutput::orphaned_claims` is asserted empty over the fixtures, and
`diff-stage` reports any claimant of any kind that the book does not reach.

Rejected: emitting a list at its first *claimed block*. It closes the orphaning and duplicates
the first item — its lines would be in the list *and* in the intro block's paragraph. Also
rejected: shrinking list membership to whole blocks. It leaves the `list_starts_at` trigger in
place, and a whole block can still hold lines before the first surviving marker, which is the
same orphan one step later.

Not in scope, recorded: **the runaway list.** The one pdfTeX list claims 451 blocks — most of a
paper. A continuation line stays in a list while `x0 >= marker.indent − tolerance`, and when the
marker sits at the body margin every body line satisfies that, so one line-initial "1." or "–"
swallows pages of prose. Its comment says "a line indented back to the body margin has left"; the
code cannot tell. That is a quality defect — the text survives, as a list — and this phase's gate
is conservation. It inflates this class's magnitude and is recorded for the quality phase.
Evidence: `openconvert diff-stage structure` on the three documents above.
Affects: `oc-structure::{lists,stage,claims}`, PHASE 7.5 items 4-5.

## 2026-09-22 · Classes `orphaned-claimant` and `contested-claim` closed — measured · Phase 7.5
Context: the fix recorded in the entry above, and what implementing it found.

The three documents the orphan class was admitted on, `diff-stage structure`, reachable:

```
                         before            orphans fixed        + one owner per block
                    lost    appeared      lost    appeared      lost    appeared
InDesign 115755   217 873        2          24      6 940         64         0
Word     115632   257 991        0       1 135          0      1 135         0
pdfTeX 2305-09065  23 228        2           1          3          1         0
```

**499 093 characters lost → 1 200, and duplication to zero.** Every remaining character is an
orphaned **note** — a note whose marker was never linked, so nothing in the text reaches it. A
different mechanism, next.

Three things implementing it found, each a correction of the plan above:

1. **Closing the orphans exposed a class they were hiding.** Once lists reached the book,
   InDesign's duplication rose from 2 to 6 940: the 124 blocks that a list *and* a table or
   caption had both taken were now emitted by both. The orphaning had been masking it, because
   an orphaned list's copy never reached the book. `structure/appeared/contested-claim` was then
   measured on the saved inventory — 44 of 95 documents, all six strata — and admitted.
   Fix: the four detectors run in precedence order, **each built from what the ones before it
   left** — notes (the zone and the font), tables (ruling), captions (an image beside them),
   lists (a line-initial marker, the weakest signal, and the one that once took 451 blocks).
   Two overlapping table regions no longer both take one block either.
2. **The precedence fix raised InDesign's loss from 24 to 3 170**, and the per-kind report
   showed why: a chapter title repeated as a running head on fifteen pages, each copy near an
   image and so a caption *candidate*. One figure bound one copy. The claim was then re-derived
   by **text equality** — every block whose text equalled that caption — so fifteen were claimed
   and one emitted. `associate_captions` already knew which block it bound (`taken[position]`)
   and discarded it. It now returns the binding, and the claim uses it. **Fourth time in this
   phase the same shape**: a second predicate standing in for a decision already made.
3. **The first invariant was wrong.** "No block has two owners" failed on `f08`: one footnote
   block holding two notes, correctly split between them. Sharing a block is fine; owning the
   same *characters* twice is the defect. Each note claim now records the note's own text rather
   than the whole block's, and the invariant is `no_text_has_two_owners` — the claims on a block
   may not add up to more than the block holds. Mutation-tested: recording the whole block
   again fails it on `f08`.

Recorded, not fixed: **the running head that reached `structure` at all.** A chapter title
repeated on fifteen consecutive pages should have been removed by `furniture`. It repeats within
one chapter of a 368-page book, which is below any whole-book repetition ratio. That is a
`furniture` class with its own evidence to gather.

The diagnostic grew two things this needed: **ORPHANED** (claimants the book never reaches) and
examples **per claimant kind** — captions are short, and ranking by length alone hid every one
of them behind paragraphs.

Evidence: `no_claimant_is_orphaned`, `no_text_has_two_owners` (mutation-tested); 491 workspace
tests green; the table above.
Affects: `oc-structure::{stage,lists,tables,figures,claims}`, `openconvert::cmd_diff_stage`.

## 2026-09-22 · Correction: an unreferenced note is not lost — the diagnostic was stricter than the book · Phase 7.5
Context: the entry above says the three documents went from 499 093 characters lost to
**1 200**, "every remaining character an orphaned note — a different mechanism, next". The
second half is wrong.

`oc_epub::content` emits every note nothing refers to as a plain `<aside>` at the end of the
spine document covering its page — "a note nothing referred to still has text, and the text has
to reach the book". So an unreferenced note is **in the book**. `StructureOutput::reachable_text`
and `reachable_claimants` counted a note as reachable only through a marker, which is stricter
than what `epub` does, and they reported notes the output contains as losses.

Found by reading `epub` before starting on "the next mechanism", which is the order this should
always go in: confirm the defect is in the output before looking for its cause.

Decision: every note is reachable, as in the book. Figures and tables are **not** given the same
treatment, because they have no such path — one the flow does not reference is genuinely not
emitted, and for them reachability really is the flow.

Corrected figure for the three documents: **499 093 → 1 character lost** (the pdfTeX paper),
and duplication zero. There is no orphaned-note class.

The lesson is the same one as the "timeout class" two days ago, from the other side: that one
was a class invented by a harness that could not see an error; this one was a loss invented by a
diagnostic that did not know the output's contract. **A diagnostic's model of the output has to
be checked against the output**, not only against the stage it instruments.
Affects: `oc-structure::stage::{reachable_text, reachable_claimants}`.

## 2026-09-22 · Defect class `structure/lost/unattached-note-text`, admitted — the page-42 defect, at last · Phase 7.5
Context: after the orphan, contested and caption fixes, the full corpus loses 3 415 characters
where it lost 1 421 777. `diff-stage` now reports **UNATTACHED** — note-zone characters no note
carried — and on the documents that still lose text it accounts for nearly all of it, in all six
strata: pdfTeX 244/244, ABBYY-scanner 113/113 and 60/60, Word 650/644, Ghostscript 40/40,
InDesign 57/57 and 194/176, unknown 364/356, 349/343, 236/230, 48/48.

The Word document is `oapen-20-500-12657-115632`, and the block is the one this phase opened
on: page 42, *"8Cornelia Koppetsch zu zitieren, bedarf einer Fußnote…"*, 644 characters. It took
two days and five other classes to reach it, because each of those was larger.

Mechanism. `note_bodies` walks the note zone line by line. A line that opens with a recognised
marker opens a note; any other line is appended to the note that is open — **and if none is open,
it is not kept**. The block is still claimed, through the notes that do open later in it, so it
leaves the flow and those lines reach nothing. The marker rule recognises a bare digit only when
its run is flagged superscript or it is followed by `.` or `)`: `8Cornelia` is neither, so
footnote 8 opens nothing, and until footnote 9's marker arrives, footnote 8 is discarded.

*How else could this arise?*

1. **Any marker the rule does not recognise** — a digit glued to its word with no superscript
   flag, a lettered note, a roman numeral, a symbol outside the set, `*)`.
2. **A footnote continued from a page whose zone was not classified as one**, so its
   continuation arrives with nothing open.
3. **A cap or filter after assembly** that removes a note while its block stays claimed —
   checked: every `Found` becomes a `Note`, and unreferenced ones reach the book as asides, so
   this path does not lose text today.
4. **The same shape in any line-oriented assembler** — a `match` whose fall-through arm has an
   `if let Some(open)` and no `else`. Lists had it too, in the form of the continuation lines
   `build_level` did not take.

Decision: **a note-zone line is never dropped.** With no note open it opens an *unmarked* note,
which carries the text; unreferenced notes already reach the book as asides, so conservation holds
by construction, and pass 3 of the linker (order within the page) may still pair it with the body
marker it belongs to. The quality question — should `8Cornelia` have been read as marker 8 — is
a heuristic and is not this phase's; recognising glued digits misfires on `3D printing`.

Not this class, recorded: three Turkish Word articles and four oapen books lose 18–612 characters
with **zero** unattached. A different mechanism, with its own evidence to gather.
Evidence: `diff-stage structure` over the 38 documents that lost text in the previous inventory.
Affects: `oc-structure::notes::note_bodies`, `NoteLinkStats`.

## 2026-09-23 · Phase 7.5 parked by the maintainer's decision; Phase 8 begins · Phase 7.5
Context: the maintainer asked for Phase 7.5 to be parked at a clean point and resumed once every
phase is implemented. Recorded here with the state it is parked in, because the plan says of this
phase that "nothing after this phase can be trusted until it holds" — Phase 10 calibrates the
LLM's decisions by McNemar comparison against the deterministic baseline.

What that baseline is, measured on the full 104-document corpus: **79 documents clean, 13 losing
2 869 characters in total, 11 not finishing within 90 s, 1 refused by I-4; zero duplicated
characters.** When the phase opened it was 0 of 14 sampled. Phase 10's comparison will be against
that baseline and must say so.

**A correction to what PROGRESS and this log said about the timeout class.** It was localised to
`structure` — "`structure` > 280 s on a 368-page InDesign volume". That was wrong. Timing
`oc_structure::stage::structure` directly on that volume: **167 ms**. Every detector is under
40 ms. The time was measured through `dump-stage structure`, which also runs the image hashing
that feeds the stage, and all of the difference was attributed to the stage. The volume draws
**10 613 images, 5 846 of them small enough to be ornament candidates**, and hashing costs
**~237 ms per image**: the first 400 took 94.8 s, so all of them would take about 23 minutes.
The timeout class is image decoding, not structure.

An attempt to batch the decoding — load each page once rather than once per image — was
implemented, passed every test, and was **reverted unmeasured**: on its one run it did not finish
the same 400 images inside the time the old code took for them. Whether that was a regression or
this machine (memory at 1.5–4.5 GB free of 15.7, suites running at half speed) could not be told
apart without an A/B run. The next step on resuming is to time, in isolation, a page load against
`get_processed_image` against the hash, on one heavy page — the 237 ms is per call, not per pixel.
`get_raw_image` is the candidate if the render is the cost; it changes hash values and therefore
needs the ornament fixtures re-measured.

Still open, in order: the timeout class (image decoding); 13 documents losing 2 869 characters,
mechanism unnamed (three Turkish Word articles and two oapen books carry most of it); switching I-1
from `emitted_text` to `reachable_text`; `dergipark-1113748` over the furniture budget; item 7.5.0,
the reading corpus; `docs/LLM_BOUNDARY.md`. Recorded as quality and not worked: the runaway list,
the running head that reached `structure`, the arXiv datestamp panic.
Affects: PROGRESS.md, PHASE 7.5, PHASE 10's baseline.
## 2026-09-22 · The v1 prompt artifacts, and three places Appendix A is not followed · Phase 8
Context: Phase 8 commits the four tasks' `system.md`, `user.tmpl`, `grammar.gbnf` and
`schema.json` under `crates/oc-ai/prompts/<task>/v1/`, as ratified R-7 lays them out. Writing
them against llama.cpp's actual grammar parser found that the appendix they are copied from
cannot be followed literally in three places.

1. **Appendix A.2's `heading_roles` grammar does not load.** llama.cpp's `parse_sequence` skips
   newlines only inside parentheses (`parse_space(pos, is_nested)`), so a top-level rule ends at
   the end of its line, and A.2's choice — continuation lines beginning with `|` — is a syntax
   error at the first `|`. The committed grammar puts the role list in parentheses. This is the
   case for test 8.14 needing a parser that implements the dialect rather than one that accepts
   anything grammar-shaped: a grammar the server refuses makes every call fail, every failure
   falls back to the deterministic answer, and the whole AI path would look like a model that is
   never needed. `oc_ai::gbnf` follows `llama-grammar.cpp` rule for rule (names are
   `[a-zA-Z0-9-]`, `\x`/`\u`/`\U` take exactly 2/4/8 digits) and is stricter in two places — a
   rule defined twice, and a bound whose maximum is below its minimum — so a grammar that passes
   here passes there.
2. **The held-out probe rides in the same call.** A.3 sends the 8–10 held-out runs as a second
   call. ARCHITECTURE §9.6 (whose example answer carries `clusters` and `holdout` together) and
   PIPELINE §8 ("ride along in the same call") both put it in the first, they outrank the plan,
   and a second call would spend one of the book's eight on a question the first could carry.
   The grammar is therefore A.2's plus an `"h"` array of at most 10 `{"i":<index>,"r":<role>}`,
   where `i` is the probe's index in the payload: a run id is page-local (2026-09-14) and would
   not name one line of a book.
3. **`system.md` is four files, not one.** Appendix A calls the prefix "one physical file,
   symlink-free, `include_str!`-ed by every task module"; ARCHITECTURE §9.2's layout — and the
   Phase 8 file list — put a `system.md` in every task directory. ARCHITECTURE wins by the
   authority order. The four copies are held byte-identical by test 8.1, which is then a test of
   something rather than of one file's equality with itself.

Decision, also: **a released prompt version is frozen, and a manifest says so.** The cache key is
`sha256(model_id ‖ prompt_version ‖ grammar_hash ‖ user message)` (ARCHITECTURE §9.4). A
template edit changes the user message and a grammar edit changes the grammar hash, but an edit to
`system.md` changes neither: under an unchanged `PROMPT_VERSION` it would be answered from cache
entries recorded against the old prefix. `crates/oc-ai/prompts/v1.sha256` pins every v1
artifact, and `prompt_artifacts_are_pinned_to_their_version` fails on any edit. A change is a
`v2` directory and a version bump.

**`llm.verse_quote_blocks_per_call = 10`** is new: ratified note N-4's "exactly 10 blocks per
call", which the `verse_quote` grammar states as a repetition bound. A test holds the bound and
the threshold equal, as another holds the `heading_roles` bound equal to
`inventory.max_clusters`.
Evidence: `grammar_files_parse_as_gbnf` fails with `line 28: expecting a rule name` on A.2's
printed form (mutation-checked); `system_prefix_is_byte_identical_across_purposes` and the pin
test both fail on one appended byte in one copy.
Affects: IMPLEMENTATION_PLAN Appendix A.1–A.3, `oc-ai::{gbnf,prompt,provider}`, `thresholds.toml`.

## 2026-09-22 · `oc-ai` depends on `oc-model` alone, and takes its numbers from its caller · Phase 8
Context: CLAUDE.md requires every constant to come from `thresholds.toml` through
`oc_core::thresholds`, and `oc-ai` needs several — the call budget, gate V's epsilons, the token
cap. The obvious edge, `oc-ai → oc-core`, is the one the crate map forbids in effect: DECISIONS.md
Appendix A lists `oc-model` as `oc-ai`'s only workspace dependency, and ARCHITECTURE §3.1 says
`oc-net` must not depend on `oc-core` — which it would, through `oc-ai`, the moment `oc-ai` did.
`oc-validate` took the `oc-text`/`oc-core` edge on 2026-09-18, but no "must not" stood in its way.
Decision: `oc-ai`'s normal dependencies stay `oc-model` and four external crates. Every number is
a parameter its caller supplies, and the caller reads `T`. `oc-core` is a **dev**-dependency, so
the tests exercise the real values; a dev-dependency does not reach `oc-net`. The same reasoning
keeps gate V's statistics inside `oc-ai` rather than borrowed from `oc-text`.
Evidence: `crates/oc-ai/Cargo.toml`.
Affects: DECISIONS.md Appendix A (upheld), ARCHITECTURE §3.1, `oc-ai`.

## 2026-09-22 · `Decision` records why the deterministic answer stood: a code, not a flag · Phase 8
Context: A8.1 wants "a `Decision` records the failure" for any refused model answer, test 8.9
asserts `fallback_used == true` on it, and ARCHITECTURE §9.1 records `fallback_used` "on the
`Decision`". IR_SKETCH's `Decision` has no such field — only `Confidence` does, and a confidence
is about how well a label is evidenced, not about whether a model was overruled.
Decision: `Decision` gains `fallback: Option<&'static str>`, the code of what made the
deterministic answer stand after an escalation — the refusing gate (`S.enum`, `S.bijection`, …)
or, in P8.5, the budget — and `fallback_used()` is `fallback.is_some()`. A code and not a boolean,
because "the model was contradicted" and "the model was never asked" are different findings a
calibration corpus has to tell apart; a code and not the `GateFailure` itself, because a failure's
detail may quote the model, the model may have quoted the book, and the report must carry none of
it (D13.9). A refused call keeps its `LlmTrace`: the report can say a model was asked, under which
prompt, and what it answered, by hash. Additive, so `ir_version` does not move (ARCHITECTURE
§4.5); the only snapshot that serialises a decision, `report__report_f07.snap`, gains
`"fallback": null` twice.
Evidence: `gate_d_records_fallback_in_decision_log`, `an_accepted_answer_is_recorded_as_the_models`.
Affects: IR_SKETCH (`Decision`, elaborated), `oc-model::decision`, `oc-ai::gates::fallback`.

## 2026-09-22 · The escalation predicates live in `oc-core`, and one bound is already wrong · Phase 8
Context: test 8.16 wants RT C4's six predicates pure and table-tested; Phase 10 plans
`oc-structure/src/escalate.rs` to call them with the evidence `structure` measures, and the
dehyphenation predicate belongs to `oc-text`. Neither crate may depend on `oc-ai`.
Decision: the predicates are `oc_core::escalation` — the crate ARCHITECTURE §3.1 already gives
"escalation" to, and one every evidence-owning stage already depends on. Each is a pure function
of an evidence struct and the thresholds, returning `Verdict::{Fires, Abstains}` with a reason a
report can carry. Gathering the evidence stays with the stage; judging it is this module's.
Found on the way: comparing the `f32` short-line ratio against the `f64` threshold by widening
the ratio puts a block exactly on the closed lower bound outside it — `f64::from(0.35f32)` is
0.3499999… — so a block with 7 of 20 short lines reads as "full lines". The predicate compares in
`f32`, and the table's lower-bound row fails with the widening restored (mutation-checked).
**`oc-structure::quotes::classify_indented` has the same widening and so the same off-by-one-ulp
bound**: at exactly `verse.short_line_ratio_min` it resolves `BlockQuote` instead of
`Ambiguous`. Recorded, not fixed — Phase 4 code outside this item; Phase 10 replaces that
comparison with a call to `oc_core::escalation::verse_quote`.
Evidence: `escalation_predicates_are_pure_and_unit_tested`.
Affects: ARCHITECTURE §3.1 and §6.1, `oc-core::escalation`, `oc-structure::quotes` (open).

## 2026-09-23 · The engine's one-argument form, and what it refuses · Phase 12
Context: DECISIONS D13.2 says "the GUI passes one argument: the job-spec path"; §2.1 of the plan
also lists `convert --job <PATH>` "when present it is the ONLY other arg allowed", which is three
arguments. §2.2 says the engine validates the spec against `schemas/job-spec.v1.json`.
Decision: the engine recognises **a lone argument naming a `.json` file** as a job spec
(`openconvert <JOB.json>`); DECISIONS wins over the plan's `--job` spelling, which is not
implemented. The lone-argument rule keeps a mistyped subcommand a usage error rather than a spec
that could not be read. The spec is validated by `oc_core::jobspec`, which **walks the committed
schema file** (compiled in) rather than re-stating its patterns and minimums in Rust, and fails
closed on a schema keyword it does not implement; the app and the engine run the same function.
A valid spec field the engine does not act on yet (`ai.enabled`, `threshold_overrides`,
`dump_stages`, and `overrides_path` until P12.9) is **refused by name**, never ignored: ignoring it
would report success on a conversion the user asked for differently. `output.overwrite = false`
(the schema default) refuses an existing output with `E_OUTPUT_EXISTS`, exit 2 — the app picks a
free "name (2).epub" before writing the spec (design decision, `result.html` §5); the CLI keeps
replacing as it always has. `input.sha256`, when given, must match (`E_INPUT_CHANGED`, exit 2).
A document that needs a password or exceeds a limit is `E_PASSWORD_REQUIRED` / `E_LIMIT_EXCEEDED`
with exit 2 from `convert` too, the codes `inspect` and `dump-stage` already use, so the app can
prompt or name the limit. `done` now carries `report_path` (§2.3), and prose no longer follows a
`fatal` on stderr when stderr is the NDJSON channel.
Evidence: `crates/oc-core/src/jobspec.rs` tests; `crates/openconvert/tests/job_spec.rs`.
Affects: IMPLEMENTATION_PLAN §2.1 (`--job` not implemented), §2.2, `oc-core::jobspec`, `openconvert`.

## 2026-09-23 · What the engine's `stage` events cover, and where cancel is checked · Phase 12
Context: Phase 12 detail 3 has the app render only what the engine reports, and UI_UX §2.2 maps
the twelve stage names to six labels. `convert` emitted one pseudo-stage, `convert`, and nothing
else; it did not listen on stdin at all, so a cancel from the app could not have worked.
Decision: `convert_observed` reports the stages it runs, by their IR names, and only those —
`inspect` and `paragraphs` are not run by the conversion driver today and are not invented. Two
attributions are choices: the ornament rule's image hashes are reported as `structure`'s work
(they are its evidence), and image decoding as `epub`'s (the first thing "Building" does). The
repair host reports `epub`, `validate` and `repair` itself, so the loop's single call is three
stages to the user, and `repair` appears only when a repair runs (UI_UX §2.2). Writing
`report.json` is the `report` stage. `progress` is coalesced in the CLI's sink, not in the stages
(ARCHITECTURE §8.3), at most `ipc.progress_max_per_sec` per stage and always including the last
unit. The heartbeat runs on its own thread for the whole run and is stopped before `done`/`fatal`
is written, so the final event stays the last line. A cancel is honoured at every stage boundary,
between pages in `ingest`, between images when hashing and decoding, and once more before anything
is written to the destination; past that point the answer is the book. Not yet checked inside
`text`/`layout`/`structure` or inside one `build_epub` call: on the 300-page reference book the
cancel lands in well under the 2 s deadline, but a single stage that runs longer than 2 s on a
very large book would overrun it — Phase 14's per-stage deadlines share this flag and are where
finer checks belong.
Evidence: `crates/openconvert/tests/progress.rs`; `events::tests::the_heartbeat_beats_while_work_runs_and_stops_before_it_returns`.
Affects: IMPLEMENTATION_PLAN §2.3, UI_UX §2.2, `openconvert::convert`, `oc-validate::repair::host`.

## 2026-09-23 · PROVISIONAL — needs maintainer ratification: the startup handshake is `--version` · Phase 12
Context: test 12.17 and A0.7 want the app to refuse a stale staged engine **at startup**, before
any job; D13.2 says "the GUI passes one argument: the job-spec path", and a job spec needs an input
and an output, which startup does not have. Phase 0's window used `inspect --json --progress json`
on a fixture, which is four arguments and a file the installed app does not ship.
Decision (provisional, the most conservative reading): the handshake runs the engine with **one
fixed argument, `--version`**. When stderr is not a terminal the engine answers with the `hello`
event on stderr (the version line still goes to stdout, so a person at a terminal sees no JSON).
The app compares `engine_version`, `protocol` and `ir_version` with its own and shows the blocking
startup error on any difference. The command is built in Rust; the webview can add nothing to it.
So the engine accepts exactly two one-argument shapes from the app: `--version` and a job-spec
path. Overturning this means either a job-spec field for a no-op "hello" job (a schema change, so
`job-spec.v2`) or checking only the first event of the first real job, which cannot block startup.
Evidence: `stale_sidecar_is_refused_at_startup` (feature `engine-integration`),
`the_handshake_refuses_every_mismatch_by_name`, `version_answers_the_handshake_with_hello`.
Affects: D13.2 (interpretation), A0.7, `openconvert` `--version`, `apps/desktop/src-tauri/src/engine.rs`.

## 2026-09-23 · The app spawns the engine from Rust, never from the webview · Phase 12
Context: Phase 0's window spawned the engine from TypeScript through `@tauri-apps/plugin-shell`,
with a capability allowing `bin/openconvert` and `"args": true` — any arguments. Phase 12's
architecture puts `spawn_engine` and the job queue in Rust.
Decision: every engine process is started by `openconvert_desktop::engine` in Rust, after the job
spec has been validated by `oc_core::jobspec` and written into the app's job directory; the
command has one argument, a path that `fs_scope::is_inside` confirms is in that directory. The
webview reaches the engine only through the app's own Tauri commands, which take file paths and
presets, never arguments. This makes the one-argument rule a property of code the UI cannot reach,
rather than of a shell-scope regex; the capability change that follows (the webview loses
`shell:allow-execute` entirely) is P12.5. Process groups and job objects remain Phase 9's; until
part B threads them in, a Unix engine gets its own process group and the supervisor kills the
engine itself, which orphans nothing while AI is off.
Evidence: `engine_is_spawned_with_exactly_one_argument`, `job_spec_is_validated_before_spawn`.
Affects: RT B15, D13.2, Phase 12 detail 1, `apps/desktop/src-tauri`.

## 2026-09-23 · The UI's warning sentences are the engine's templates · Phase 12
Context: the design's string inventory carries `warn.*` drafts in EN/DE/TR, two of them for codes
the engine does not have and several with arguments the engine does not send
(`W_TABLE_AS_IMAGE` with `{n}`, where the engine sends `{rows}`, `{columns}`, `{reason}`). The engine
already has a template for every registered code in all three locales
(`crates/oc-core/src/warnings/templates_*.toml`), held to the registry by `xtask ci-lint` and by
four tests, and the CLI renders from them.
Decision: the UI renders warnings from **those same files**, compiled into the bundle at build time
(`ui/src/lib/warnings.ts`), and the locale JSON files carry no `warn.*` keys. One sentence per code
per locale in the repository, so the argument names cannot drift between the two front ends, and a
new code needs its three templates exactly once. The design's warning copy is therefore not used;
its phrasing can be adopted by editing the TOML templates, where it will also reach the CLI. Test
12.9 reads the registry and asserts every code has a template in every locale with the English slot
set, and that the UI's three JSON files have identical key and slot sets. Test 12.8 uses the real
arguments of `W_TABLE_AS_IMAGE` rather than the plan's illustrative `{count:3}`. There is no
English fallback anywhere: a missing key renders as a visible `⟦key⟧` marker (A12.3).
Evidence: `warnings_are_localised_from_code_and_args`, `every_warning_code_has_every_locale`.
Affects: docs/design/handoff/docs/strings.md (`warn.*` rows superseded), Phase 12 rows 12.8, 12.9.

## 2026-09-23 · The preview is the EPUB served to a sandboxed frame by an app protocol · Phase 12
Context: Phase 12 detail 7 wants a normal, visible webview rendering the generated XHTML, labelled
approximate; the design draws it as route `preview` inside the main window with chapter navigation
and a fixed label. The main window runs under `default-src 'self'; connect-src 'none'`, has IPC
access, and must never execute or style itself from a book's markup.
Decision: the app registers one custom protocol, `ocpreview`, that serves the finished EPUB of a
job, entry by entry, straight from the archive (`src-tauri/src/preview.rs`): only names the archive
contains, nothing with `..`, `/`-rooted or `\`; every response carries its own CSP that allows no
script and nothing outside the book, plus a small stylesheet that marks `:target` in system colours
so a page link lands visibly. The main window shows it in an `<iframe sandbox="">` — no scripts,
an opaque origin, no IPC, no top navigation — and its CSP gains exactly `frame-src ocpreview:
http://ocpreview.localhost` (WebView2 spells custom schemes as `http://<scheme>.localhost`, which is
served in-process and never reaches a network). The privacy test pins that as the only non-'self'
source in the policy. The page list in the book's navigation document drives page steps, so "p. 34"
means the book's own page 34. The design's highlighted `<mark>` becomes the `:target` of the page
anchor; the block-level highlight a warning's `block_ids` could drive needs the engine to anchor
blocks by id in the XHTML, which it does not today. Unverified here: rendering in WebKitGTK, WKWebView
and WebView2 (no display on this machine; the component tests run in jsdom).
Evidence: `preview::tests`, `webview_has_no_network_permission`, `preview.svelte.test.ts`.
Affects: Phase 12 detail 7, D13.9 (CSP, narrowed widening), UI_UX §2.3, report.html §3.

## 2026-09-23 · User corrections are applied by `document` and draw on no budget · Phase 12 · PROVISIONAL — needs maintainer ratification
Context: the documents disagree about which stage cites `UserOverride`. PIPELINE §9 (step 7, and the
stage's Purpose) and ARCHITECTURE §4.7 apply `overrides.json` in `document` — "after `structure` and
before `document`", "overrides applied last" — while PIPELINE §2's stage table and IR_SKETCH give
`UserOverride` to `repair` ("Conserving, except `UserOverride`"), and `thresholds.toml` counted it in
the `other` budget, which ARCHITECTURE §4.7 contradicts ("the one place a human may overrule the
conservation budgets"). DECISIONS.md is silent on the owner. A renamed heading is text inside `C`,
so the choice decides which invariant check sees it.
Decision (provisional): `document` applies the corrections as its last step and is checked under a
second declared contract, `stages::DOCUMENT_CORRECTED` = Budgeted{`UserOverride`}, used only when a
job names an `overrides.json`; a run without one keeps `document` Conserving. `repair` stays
Conserving (it never sees a correction). `UserOverride` may add text (`Reason::may_add`: a renamed
heading's new words), and draws on no budget group and not on the global non-OCR cap; I-1, I-2 and
I-7 apply to it in full. The corrections' ledger entries go into the document's ledger before the
validate→repair loop, so the loop's I-7 balances with them. A file that does not apply — another IR
version, another PDF, block-level entries (D16), unreadable — converts the book without it and is
named by a warning (`W_OVERRIDES_STALE{file_ir, engine_ir}`, `W_OVERRIDES_OTHER_SOURCE`,
`W_OVERRIDES_BLOCKS`, `W_OVERRIDES_UNREADABLE`); a TOC correction naming a heading the book does not
have is counted in `W_OVERRIDES_UNMATCHED{count}`. A level change re-nests the section tree by level
in reading order, and a moved section takes the role its place implies (a chapter under a chapter
becomes a section; a section moved to the top becomes a chapter; front and back matter keep theirs).
Every applied correction is a `Decision{method: User}` whose alternative is what the pipeline chose.
The report gains `document.authors` and `document.toc` (heading id, title, level, printed page) — the
editors' starting point — and `convert` gains `--overrides <PATH>` (§2.1 lists it).
To ratify: the owner (document vs repair) and the budget exemption; PIPELINE §2's table, IR_SKETCH's
stage-kind line and the `repair.rs` comment would then be aligned.
Evidence: `overrides_with_wrong_ir_version_are_refused` (12.11), `overrides_roundtrip_metadata_and_toc`
(12.10), `a_stale_overrides_file_is_named_and_the_book_converts_without_it`,
`a_user_override_is_balanced_but_draws_on_no_budget`.
Affects: PIPELINE §2 and §9, IR_SKETCH stage kinds, ARCHITECTURE §4.6–4.7, thresholds.toml comment.

## 2026-09-23 · "Fix and rebuild" resumes from a per-book save named by `OC_CACHE_DIR` · Phase 12 · PROVISIONAL — needs maintainer ratification
Context: R-15 ratified the partial re-run — a correction re-runs only `document` → `epub` →
`validate` → `repair` → `report` "from the cached `structure` output" (A12.4b) — but no document says
where that cache lives, what it holds, who names it, or when a run may resume from it. The job spec
(schemas/job-spec.v1.json) has no field for it, and the save holds the book's text, which SECURITY
§10 treats as something to disclose and make clearable.
Decision (provisional): the engine saves what `structure` settled — `Upstream`: the sections, notes,
figures, tables, metadata and warnings `document` reads, the per-page labels, classes, orientation and
column counts, the block→page map, the image list, the producer family and the ledger through
`structure` — as `<OC_CACHE_DIR>/structure/<source_sha256>.json`, keyed by engine version, IR version,
digest and forced language, after every full run **when and only when** the environment variable
`OC_CACHE_DIR` names a directory (the CLI writes nothing by default). A run that brings corrections
(`overrides_path` / `--overrides`) and finds a save whose key matches starts at `document` from it,
with the budget totals replayed from the saved ledger (`ReasonTotals::replay`); any mismatch or
damage is a full run, never an error. The images are still decoded from the PDF for `epub` — that is
`epub`'s work, not a stage before `structure`. The IR types gained `Deserialize` (plus `BlockId`
from its text and `CharHistogram` from its map); the three `&'static str` fields are saved as text
and read back against the warning registry and the stage declarations, so an unknown code or stage
is a miss. A variable rather than a job-spec field because the location is the app's, not the job's
(like `OC_PDF_PASSWORD`) and the committed v1 schema stays as the plan wrote it. The desktop app sets
it to its own `cache/` directory and clears saves with the rest of its cache.
To ratify: the variable (vs a job-spec field) and the save's lifetime (the app clears it at start).
Evidence: `a_rebuild_runs_only_the_stages_after_structure` (A12.4b: only downstream stage events; the
rebuilt EPUB is byte-identical to a full run with the same corrections, the report equal but for
timings), `a_save_that_does_not_fit_is_a_full_run`, `replaying_a_ledger_charges_what_the_stages_charged`,
`a_histogram_reads_back_as_written`, `a_block_id_reads_back_from_its_text_and_only_from_it`.
Affects: UI_UX §2.3, IMPLEMENTATION_PLAN Phase 12 detail 8 / A12.4b, SECURITY §10 (a second on-disk
store of document text, in the app's cache directory).

## 2026-09-23 · The editors keep corrections in the app's data directory and propose nothing · Phase 12
Context: result.html §2–3 draws the metadata and TOC editors, the partial rebuild, the updated
result and the stale-corrections refusal. Its stale frame shows the corrections file beside the
book (`~/Books/.openconvert/<book>/overrides.ir3.json`) with a "Show file" button, and its TOC frame
says merge, move and delete "are saved as proposals for the report" — but draws no control that
makes one.
Decision: corrections live in the app's own data directory, `overrides/<source_sha256>.json`, one
file per book, never beside the user's files; an editor sends only what differs from the report it
shows, and the Rust side merges it into the book's file (a second correction keeps the first,
because each rebuild starts from the book as `structure` left it). "Fix and rebuild" replaces the
row with a rebuild job that names the corrections and the digest they were made for (a PDF changed
since is refused, `E_INPUT_CHANGED`) and replaces the EPUB the queue wrote. The rebuild row shows
only the steps it runs (Reconstructing, Building, Checking) and says "from cache" when the engine
resumed after `structure`; the finished row says what was applied, from the rebuilt report's
`method: user` decisions. A refused corrections file is the engine's warning sentence in a banner —
no "Show file", since the file is the app's, not the user's. The TOC editor renames and re-levels
only; no proposals are made or promised (the help text says what the editor does). The language
list is the book's current tag plus en/de/tr, named by `Intl.DisplayNames` in the UI's language,
"detected" only when no correction set it. The engine's cache (`OC_CACHE_DIR`) is the app's
`cache/`, emptied at every start.
Evidence: `editor.svelte.test.ts` (3), `a_second_correction_keeps_the_first`,
`a_rebuild_replaces_the_row_with_the_corrections_named`, `the_engine_is_told_where_the_cache_is`,
`the_cache_is_cleared_and_corrections_are_kept_by_digest`.
Affects: result.html §2–3 (design decision 6: a small overturn, recorded here), UI_UX §2.3.

## 2026-09-23 · Settings › Advanced offers only what the engine honours · Phase 12
Context: UI_UX §2.4 and settings.html list, under Advanced, thread count, max pages, max memory,
Clear cache, Dump stages, a document-language override and a one-job PDF password; UI_UX §2.4 also
names a `fast|balanced|thorough` quality preset. The v1 job spec (`schemas/job-spec.v1.json`) has
no thread, document-language or quality field, and the engine refuses `dump_stages` by name.
Decision: Advanced shows max pages and max memory (→ the spec's `limits`, defaults from
`thresholds.toml`) and the cache (size and book count, "Clear cache…" behind one confirmation,
SECURITY §10). Omitted, each because a control the engine cannot act on would be a fake one: the
thread count, Dump stages and the document-language override (no spec field / refused field — a
spec change is the maintainer's call), the quality preset (no overlays in `thresholds.toml` yet),
and the one-job password field (the locked row's inline prompt already delivers D13.11's one-job
password, through the environment). The first-run card and route are hidden until `ModelReadiness`
exists (Phase 9, part B).
Evidence: `settings.svelte.test.ts` (4), `the_cache_is_cleared_and_corrections_are_kept_by_digest`,
`axe_has_no_serious_violations` (settings and its Advanced section).
Affects: UI_UX §2.4, settings.html §Advanced, Phase 12 part B.

## 2026-09-23 · Phase 12 in CI, and the signing dry run that could not run here · Phase 12
Context: Phase 12's tests span three toolchains (the Tauri crate, the Svelte UI, Playwright), and
detail 13 asks for the Phase-15 signing and notarization workflow to be run once on a throwaway tag
(row 12.14 `signing_dry_run_completes_on_a_throwaway_tag`, A12.7, ratified R-17).
Decision: ci.yml's `desktop` job now also lints with `--all-features` and runs the desktop crate's
tests with `engine-integration` against the real engine (12.1, 12.2, 12.6, 12.7, 12.13, 12.17 and
the binary-anchored A12.1); the `ui` job runs Vitest, svelte-check, the UI lint, the build, and the
Playwright spec under the shipped CSP in Chromium (12.14–12.16, A12.5); nightly.yml gains
`webkit-ui`, the same spec in WebKit. `.github/workflows/signing-dryrun.yml` is the dry run:
manual only, `v0.0.0-*` tags only, the three platforms' bundles (NSIS + MSI, app + DMG, AppImage)
with updater artefacts, every Mach-O in the macOS bundle verified with `codesign --strict`, the app
with `spctl --assess` and `stapler validate` (app and DMG), Authenticode verified with `signtool`,
every updater signature verified against the public key, and the tag deleted whatever happened. The
bundle settings Phase 15 will own are passed as `--config` overrides rather than committed.
Outcome: **not run — unverified here.** This machine has no GitHub Actions, no signing certificates
and no macOS or Windows; the workflow's YAML parses and nothing more is known. Until a maintainer
runs it with the secrets it names, A12.7 is open. The bundle does not yet carry `libpdfium`
(Phase 15) or `llama-server` (Phase 9); the nested-signature walk covers them unchanged once they
are packaged.
Evidence: the workflows; `forty_dropped_books_all_complete_one_at_a_time` (A12.1, real engine).
Affects: IMPLEMENTATION_PLAN Phase 12 detail 13, A12.1, A12.5, A12.7; Phase 15.

## 2026-09-23 · `webpki-roots` resolved by option (a): platform roots · Phase 9
Context: the 2026-09-09 entry deferred the choice between (a) platform/native TLS roots, (b) a
`deny.toml` exception scoped to `webpki-roots`, and (c) amending D15, to the phase that owns
downloads.
Decision: (a). `ureq = { default-features = false, features = ["rustls-no-provider",
"platform-verifier"] }`, and `rustls` named beside it with only the `ring` provider, so no private
`_ring` feature is used. `HttpFetch` sets `RootCerts::PlatformVerifier` and the ring provider
explicitly; with `rustls-no-provider` and neither set, ureq would panic at the first TLS handshake.
Evidence: `cargo deny --all-features check` → `advisories ok, bans ok, licenses ok, sources ok`;
`cargo tree -p oc-net -e normal --target <t> -i webpki-roots` finds no such package on
`x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc` or `aarch64-apple-darwin`.
`rustls-platform-verifier` 0.7 depends on `webpki-root-certs` for Android only, which is not a
`deny.toml` target. `deny.toml` is unchanged: `exceptions = []` holds.
Affects: D15, `Cargo.toml`, `crates/oc-net`.

## 2026-09-23 · The allowlist is checked on every hop; the CDN host is unverified here · Phase 9
Context: a Hugging Face `resolve/<commit>/<file>` URL answers with a redirect to a CDN. The plan's
`HOST_ALLOWLIST` names `huggingface.co`, `cdn-lfs.huggingface.co` and `cdn-lfs-us-1.huggingface.co`.
Decision: `ureq` follows no redirect itself (`max_redirects(0)`). The downloader follows each
`Location` and checks it against the allowlist *before* the fetch that would open a socket to it,
up to `max_redirects` hops, so a redirect off the list is refused and its host never contacted
(test 9.3 counts the requests). The parse is deliberately narrow: `https`, a bare host, no
user-info, no port. The list is the plan's, unchanged.
**PROVISIONAL — needs maintainer ratification:** this sandbox's egress policy refuses
`huggingface.co` (HTTP 403 on CONNECT), so which CDN host a real download redirects to today could
not be observed. If Hugging Face now serves these repos from a host the list does not name,
`model pull` fails with `HostNotAllowed` naming that host. It fails closed and installs nothing.
Widening the list is then a one-line reviewed change.
Evidence: `crates/oc-net/tests/download.rs`; the proxy log for this session
(`connect_rejected … huggingface.co:443`).
Affects: D13.9, SECURITY §8, `crates/oc-net/src/allowlist.rs`.

## 2026-09-23 · A model whose licence text is not bundled is refused · Phase 9
Context: D9 and LICENSE_AND_DEPENDENCIES §5 require `LICENSE` and `NOTICE` beside every
downloaded model. The registry carries a licence *name*; the only text anyone can write is text
the app ships.
Decision: `oc-net` bundles the Apache-2.0 text (`crates/oc-net/licenses/Apache-2.0.txt`, the
repository's own `LICENSE`). A registry entry under any other licence is refused with
`NetError::UnknownLicense` before a byte is fetched. Every v1 entry is Apache-2.0. `NOTICE` is the
entry's `notice_text` (or `<display_name>, <license>.`) followed by the URL, repository, revision,
file and SHA-256 that were installed.
Evidence: `download_writes_license_and_notice`.
Affects: D9, LICENSE_AND_DEPENDENCIES §5, `crates/oc-net/src/download.rs`.

## 2026-09-23 · `HttpTransport` never uses a proxy · Phase 9
Context: `ureq` reads `HTTPS_PROXY`/`ALL_PROXY` from the environment by default. The model
downloader should honour that, because a user behind a corporate proxy cannot otherwise download
anything. `HttpTransport`, the `oc_ai::Transport` that carries a book's inventories to a model,
is a different case.
Decision: `HttpTransport` sets `proxy(None)`. The engine-owned sidecar is on loopback, and a proxy
configured for downloads is not a party that may read a book's text: D13.9's privacy claim is that
text never leaves the machine unless the user sends it to an endpoint they named. The downloader
(`HttpFetch`) keeps the environment's proxy, since what it sends is a pinned public URL. Whether a
non-loopback endpoint may be used at all is D10's consent question and Phase 11's.
Evidence: `crates/oc-net/src/transport.rs`; `http_transport_posts_json_with_the_bearer_key`.
Affects: D10, D13.9, SECURITY §8.

## 2026-09-23 · The sidecar's key goes in `LLAMA_API_KEY`; its port is picked in `oc-net` · Phase 9
Context: D8 says `llama-server` is spawned "with a per-run `--api-key`", and row 9.12 says the
spawned command line must contain no key material ("key passed via file/env"). The plan also puts
port picking in `oc-core/src/sidecar/portpick.rs`.
Decision: (1) `oc_core::sidecar::llama::command` passes the key in the environment variable
`LLAMA_API_KEY`, which llama.cpp's argument parser binds to `--api-key`, and never on the command
line. Any local user can read a process's argv with `ps`; only the same user can read its
environment. (2) Port picking binds a listener on `127.0.0.1:0` to learn a free port, and binding a
socket is opening one, which D13.9 and test 9.7 reserve for `oc-net`. It is
`oc_net::loopback::free_port`, and the caller passes the port to `oc-core`.
**PROVISIONAL — needs maintainer ratification:** that `LLAMA_API_KEY` is read by the pinned build
could not be checked here: github.com release assets are refused by this sandbox's egress policy,
so no llama-server binary was run. If the pinned build does not read it, an owned server starts
with no key. The live tests (9.15/9.16) and G1 would then see requests without a key accepted,
and test 9.8 asserts the stub rejects them. `--api-key-file` is the fallback D8's wording allows.
Evidence: `api_key_never_appears_in_argv`, `free_port_is_an_ephemeral_loopback_port`.
Affects: D8, D13.9, PHASE 9 details 2–3, `crates/oc-core/src/sidecar/llama.rs`,
`crates/oc-net/src/loopback.rs`.

## 2026-09-23 · `llm.cache_reuse_min_chunk = 256`, provisional · Phase 9
Context: PHASE 9 detail 2 passes `--cache-reuse N` to entries whose `models.toml` says
`cache_reuse = true`, and names no N.
Decision: 256 tokens, `source = provisional`. The value is the one llama.cpp's examples use. It is
not measured here. Gate G5 measures the consequence directly: a second identical call set must cost
≤ 40 % of the first.
Affects: `thresholds.toml`, `crates/oc-core/src/sidecar/llama.rs`.

## 2026-09-23 · Who tears the sidecar down, and what is left for Phase 14 · Phase 9
Context: PHASE 9 detail 5 and D13.2 ask that an engine-owned `llama-server` never outlive the
engine: `setsid` + `kill(-pgid)` and `PR_SET_PDEATHSIG` on Unix, a nested job object with
`KILL_ON_JOB_CLOSE` on Windows, SIGTERM/SIGINT and console-ctrl handlers everywhere, and "a `Drop`
guard alone is insufficient". CLAUDE.md requires `#![forbid(unsafe_code)]` in every crate except the
PDFium binding.
Decision:
1. `oc_core::sidecar::supervise` owns every child in one registry, and four paths tear it down:
   `OwnedServer`'s `Drop` (normal exit), a **panic hook** (runs before unwinding and before a
   `panic = "abort"` abort), a **signal handler** via `ctrlc` (SIGINT/SIGTERM/SIGHUP on Unix,
   Ctrl-C/Break/close on Windows), which kills the children and exits with `ExitCode::Cancelled`
   (3), and the supervisor's own group kill. Teardown is `Child::kill` then `wait`, so nothing is
   left as a zombie.
2. **The server stays in the engine's process group**, and is not `setsid`'d into its own.
   ARCHITECTURE §8.2 has the *app* `setsid` the *engine* and kill it with `kill(-pgid)`, and that
   group kill reaches the server only if the server is in the engine's group. A server in a group
   of its own would survive exactly the supervisor kill the design relies on.
3. **PROVISIONAL — needs maintainer ratification:** `PR_SET_PDEATHSIG` has to be set in the child
   between `fork` and `exec` (`CommandExt::pre_exec`, an `unsafe fn`), and a Windows job object is
   FFI. Neither can be written under `forbid(unsafe_code)`. So an engine killed outright
   (`SIGKILL` of the engine's pid alone, or a segfault in PDFium) can still orphan its server on
   Linux and Windows. Both land with Phase 14's hardening, either through a reviewed wrapper crate
   or a re-exec trampoline that calls rustix's safe `set_parent_process_death_signal` and then
   `exec`s the server. When the app is the supervisor, the group kill covers the Unix case already.
4. The lifecycle tests use two dev-only binaries in `oc-testkit`: `oc-stub-llama-server` (the real
   server's command line, `/health` without a key, `/v1/chat/completions` only with one) and
   `oc-sidecar-engine` (drives `oc_core::sidecar` exactly as the engine will). They live there
   because `CARGO_BIN_EXE_*` is visible only to a package's own tests, and Appendix A already makes
   `oc-testkit` the home of "a stub LLM server for tests".
5. Idle-kill is a policy on `OwnedServer` (`call_started`, `call_finished`, `kill_if_idle(now)`),
   handed its clock by the caller (row 9.13). The loop that calls it belongs to whoever makes calls,
   which is Phase 10.
Evidence: `owned_server_is_killed_on_engine_exit`, `…_on_engine_panic` (the engine forgets its
server, so only the hook can act), `…_on_engine_sigterm`. Mutation: with `kill_all()` removed from
the hook and the handler, the panic and SIGTERM tests fail and the exit test (`Drop`) still passes.
25 consecutive runs green after fixing a race in the harness (the engine now waits for a go line
before it ends). `cargo check -p oc-core` passes for `x86_64-pc-windows-msvc` and
`aarch64-apple-darwin`. The tests themselves have run on Linux only.
Affects: D8, D13.2, ARCHITECTURE §8.2, SECURITY §3, PHASE 9 detail 5, Phase 14.

## 2026-09-23 · `openconvert model`: what it reads and what a first-run screen gets · Phase 9
Context: PHASE 9 details 6–7 and §2.1 specify `model pull|list|remove` and a `ModelReadiness` per
registry entry with "a CPU expectation string ("first call ~5–15 s on a 4-core laptop")". UI_UX §2
words the same field as "fast" / "moderate" / "slower, higher quality".
Decision:
1. The registry is **compiled into the binary** (`include_str!` of `models.toml`), per D9's "a hash
   compiled into the app's model registry". `--registry` replaces it for tests and for a maintainer.
2. `cpu_expectation` is a new optional `models.toml` field, written in UI_UX §2's categories:
   default "moderate", 0.6B "fast", 4B "slower, higher quality", the experimental entry "not yet
   measured". No timing is shipped, because none has been measured on machine L. The plan's
   seconds are G4's to establish.
3. `ModelReadiness` is in `oc_core::sidecar::readiness` (types only, as the plan places it) and
   carries the plan's seven fields plus four the registry already has and a model manager row
   shows (UI_UX §2): `display_name`, `tier`, `is_default`, `warn`.
4. `remove` reads no registry, so whatever is on disk can always be deleted. An absent model is
   exit 0 with "not installed; nothing to remove" (row 9.19). A registry that does not load is
   exit 2 (`E_MODEL_REGISTRY`), an unknown id exit 2 (`E_MODEL_UNKNOWN`), and a failed download
   exit 1 (`E_MODEL_DOWNLOAD`). Download progress is a `progress{stage:"download", unit:"bytes"}`
   event at most once per percent.
**PROVISIONAL — needs maintainer ratification:** the shipped `models.toml` still has its `TODO_`
pins, because this sandbox cannot reach huggingface.co (see the registry entry below). Until they
are filled, `model list` and `model pull` on the bundled registry exit 2 naming the placeholder.
That is the refusal row 9.1 asks for, applied to the real file.
Evidence: `model_list_json_shape` (snapshot), `model_remove_absent_exits_zero_with_a_message`,
`model_pull_refuses_a_host_off_the_allowlist`, `an_unresolved_registry_is_a_usage_error`,
`an_unknown_model_id_is_a_usage_error`.
Affects: D9, UI_UX §2, IMPLEMENTATION_PLAN §1.6 and §2.1, `crates/openconvert/src/cmd_model.rs`.

## 2026-09-23 · llama.cpp pinned at `b10456`, digests unfilled; live tests in `oc-testkit` · Phase 9
Context: PHASE 9 detail 1 asks for `xtask fetch-llama-server` to download a pinned `b<N>` release
asset and check it against a SHA-256 in `xtask/llama.lock`. Rows 9.15 and 9.16 are live tests,
nightly, behind `--features live-llm`.
Decision:
1. `xtask/llama.lock` pins **`b10456`**, the release V1 §3 read on 2026-08-17 and whose asset list
   it recorded (ubuntu-x64 and macos tarballs, win-cpu-x64 zip, one CPU build per arch).
   **PROVISIONAL — needs maintainer ratification:** the digests and sizes are `TODO_SHA256` / `0`.
   This sandbox's egress policy refuses github.com release downloads (HTTP 403), so no asset could
   be hashed here, and a digest copied from anywhere but the asset itself would make the check
   circular. `fetch-llama-server` refuses an unfilled entry by name. Until someone hashes the three
   assets, the nightly live job fails at its first step, which is the honest state of the gate.
2. The archive is unpacked whole and `llama-server` is found by name, because a release carries its
   backend shared libraries beside the binary (`GGML_BACKEND_DL`) and V1 could not confirm the path
   inside the archive. Staging it as a Tauri `externalBin` is packaging (Phase 15).
3. The live tests are `crates/oc-testkit/tests/live_llm.rs`, not `oc-ai`'s: they need `oc-net`'s
   `HttpTransport`, and test 8.15 walks `oc-ai`'s dev-dependencies too. They reuse Appendix A.3's
   worked examples from `crates/oc-ai/tests/common/` through `#[path]`. With the feature on and
   `OC_LLAMA_SERVER`/`OC_LIVE_MODEL` unset they fail, never skip. Pointed at the stub server here,
   9.15 passed and 9.16 failed with `W_LLM_PREFIX_COLD` (the stub reports `cache_n: 0`). So the
   harness runs end to end and the cold path fires; neither says anything about a real model.
4. `W_LLM_PREFIX_COLD` (Warn, args `call` and `task`) is `oc_ai::prefix::check`: calls after a
   book's first must report cached prompt tokens (`timings.cache_n`, or
   `usage.prompt_tokens_details.cached_tokens`), and a reply that reports neither counts as cold.
   Re-checking the wall-clock share when it fires is the call loop's job, Phase 10.
Evidence: `an_unfilled_digest_is_refused_before_anything_is_fetched`,
`prefix_cold_is_warned_after_the_first_call`, `cached_prompt_tokens_are_read_from_the_reply`,
`test_the_live_llm_job_runs_the_live_tests_against_a_fetched_server_and_model`.
Affects: D8, D9, PHASE 9 details 1 and 4, `.github/workflows/nightly.yml`.

## 2026-09-23 · The nine-gate harness, and what it cannot yet decide · Phase 9
Context: PHASE 9 detail 9 and D9 specify `eval/model_gate.py`: G1–G9 on reference machines L and M
against the pinned llama.cpp build, results as JSON under `eval/results/model_gate/`, and
`docs/MODEL_GATE.md` generated from them. Row 9.17 asks that the script print a full pass/fail
table and exit non-zero on any failure. Row 9.20 asks that `models.toml`'s default change only with
nine green gates.
Decision:
1. `eval/model_gate.py` is a thin entry point over `oc_eval.model_gate` (`gates`, `server`,
   `results`, `mcnemar`, `probes`, `fixtures`, `registry`, `cli`). **A gate whose inputs are missing
   reports `not_run` and names the missing input, and `not_run` is never a pass.** The verdict is
   `pass` only when all nine pass. Mutation: counting `not_run` as a pass turns
   `test_a_gate_that_did_not_run_is_never_a_pass` red.
2. **G8's probes are generated, not authored by a native speaker**: 100 German and 100 Turkish
   lines from reviewable template pools with a fixed seed, labelled with the `heading_roles` enum
   (a test holds the probe enum equal to the grammar's `role` rule), 100 distinct lines per
   language, at least 11 per role. Each label follows from the template that made the line.
   Quotations are ones whose attribution is well established, or proverbs labelled as such.
3. **G2–G5 ask 20 prompt fixtures**: the renderer's own four (read from the committed cassettes)
   and sixteen composed in the same templates and compact JSON. G2's semantic assertions are the
   task-level ones (metadata copied verbatim, every cluster and holdout id answered once, book
   structure indices in range and increasing, verse ids bijective). G4 and G5 sum each call's own
   wall clock.
4. **PROVISIONAL — needs maintainer ratification:** G3's tokenizer half needs reference
   tokenizations of the 20 fixtures from the model's official tokenizer at the pinned revision,
   which could not be fetched here. `reference_tokens` is `null`, so G3 reports `not_run`. G7 needs
   Phase 10's paired answers (`--g7-pairs`). G9 needs a GGUF we converted and quantised ourselves
   (`--g9-sha256`). G4's share needs the reference book's `--no-ai` wall clock on the same machine
   (`--deterministic-seconds`). G7's level and non-inferiority margin are new provisional
   thresholds (`model_gate.g7_alpha` 0.05, `model_gate.g7_noninferiority_margin` 0.02), because D9
   names McNemar and gives neither.
5. McNemar is computed exactly (`math.comb`), and the one normal quantile comes from
   `statistics.NormalDist`. There is no scipy import, in line with `oc_eval.metrics.assertions`,
   and no untyped dependency for mypy.
6. `--emit-registry` resolves each repository's commit, asserts the file is in that commit's tree
   (V1 §1(g)), streams the file through SHA-256 and writes the pins only if our hash and size agree
   with the hub's LFS record. Nothing is written on any refusal. It is tested against a fake hub
   only: huggingface.co is refused by this sandbox's egress policy.
7. Row 9.20 is `test_default_stays_qwen3_1_7b_until_gate_passes` in `eval/tests/test_model_gate.py`,
   with `registry.default_problems` as the rule: the default must be a `tier = "default"` entry, and
   anything other than Qwen3-1.7B needs its latest run passing on both L and M.
**No gate result is recorded.** This box is neither L nor M and has no server or model, and a result
file from it would be a claim about a machine D9 does not recognise. `docs/MODEL_GATE.md` says so,
and Qwen3-1.7B stays the default. Nothing promotes Qwen3.5-2B.
Evidence: `eval/tests/test_model_gate.py` (19 tests), `docs/MODEL_GATE.md`.
Affects: D9, RT C3, PHASE 9 rows 9.17, 9.20, detail 9, `thresholds.toml` (`model_gate.*`).

## 2026-09-23 · The desktop app's own server (`llm.rs`) moves to Phase 12 · Phase 9
Context: PHASE 9's file list includes `apps/desktop/src-tauri/src/llm.rs`, the app-owned,
long-lived `llama-server` that loads a model once per batch (detail 3). The desktop app is a
21-line hello-Tauri today. Phase 12 (Desktop UI) is being built at the same time on its own branch
(`phase/12-desktop-ui`), which rewrites the app's `main.rs`, manifest and process model.
**PROVISIONAL — needs maintainer ratification:** `llm.rs` is not written in Phase 9. Everything it
needs exists and is tested: `OwnedServer`, `supervise`, `LlmEndpoint::choose`,
`oc_net::loopback::free_port`, `HttpTransport`. Adding it to the app here would edit the files the
concurrent Phase 12 branch is rewriting, for code nothing can exercise until the app's model
manager exists. Phase 12 wires it, with the engine receiving the app's server through
`--llm-endpoint` and `--llm-api-key-file`. Those two `convert` flags arrive with Phase 10's use of
a model, not before, because a flag that does nothing is worse than no flag.
Affects: PHASE 9 detail 3, Phase 10 (CLI flags), Phase 12 (`apps/desktop/src-tauri/src/llm.rs`).

## 2026-09-23 · llama.cpp `b10456` digests filled, then checked by download · Phase 9 follow-up
Context: the entry "llama.cpp pinned at `b10456`, digests unfilled" left `xtask/llama.lock` with
`TODO_SHA256` and `size_bytes = 0` for all four assets. `fetch-llama-server` refused to run, so the
nightly live job failed at its first step.
Decision:
1. The four digests and sizes are the `digest` (`sha256:…`) and `size` fields that GitHub's releases
   API reports for each asset (`api.github.com/repos/ggml-org/llama.cpp/releases/tags/b10456`). They
   were read twice, and the two reads matched.
2. **Then they were checked by download.** When this follow-up ran, the sandbox's proxy let
   github.com release downloads through, which the Phase 9 run had found refused.
   `cargo run -p xtask -- fetch-llama-server` fetched the ubuntu-x64 asset, `verify` accepted its
   size and SHA-256, and it unpacked the archive. The unpacked `llama-server --version` reports
   build 10456, commit `f275595dd`. All four assets were then downloaded with `curl` and hashed
   with `sha256sum`, and each digest and byte size matches the lock:
   ubuntu-x64 `d07b3f80…c577` 16 645 205, macos-arm64 `5ab514e2…eb6f` 11 072 436, macos-x64
   `5913d397…f489` 11 379 321, win-cpu-x64 `52ea16a7…2a2d` 18 464 144. The sizes also agree with
   V1 §3's 10.6 / 15.9 / 17.6 MB.
3. The digest and the download both come from GitHub, so the pin records what GitHub served on
   2026-09-23. It catches a corrupted transfer or a later change to the asset. It cannot catch a
   release that was already bad when it was uploaded. The PDFium and EPUBCheck pins have the same
   limit.
4. The pinned build's `--help` lists `(env: LLAMA_API_KEY)` under `--api-key`, which is the
   variable `oc_core::sidecar::llama::command` sets. Whether a running server enforces the key
   still needs a model to test, and none can be fetched here.
**PROVISIONAL — needs maintainer ratification: verify by downloading.** Run
`cargo run -p xtask -- fetch-llama-server` on a maintainer machine. It verifies that host's asset
against the lock. The download check above was done by an unattended run, not by a maintainer
reviewing the pin. The lock's header says moving a pin is a reviewed commit, and this commit fills
the pin for the first time.
Still blocked: `models.toml`. huggingface.co is still refused (`CONNECT` 403, checked again with
`curl` for this entry), so the registry pins stay `TODO_`. The nightly live job now gets past the
server fetch and fails at `model pull`.
Evidence: `the_shipped_lock_pins_all_four_assets_by_sha256_and_size` (asserts four distinct
lower-case SHA-256s and non-zero sizes, all accepted by `pinned`), and the `sha256sum` / `stat`
output above.
Affects: D8, D9, PHASE 9 detail 1, `xtask/llama.lock`, PROGRESS.md Blocked items 2 and 3.

## 2026-09-23 · The app's own model server: one per app, a key file, idle by job · Phase 12
Context: PHASE 9 detail 3 says "the desktop app supplies a long-lived, app-owned server so a 1 GB
model loads once per batch, not once per book", and files `apps/desktop/src-tauri/src/llm.rs` under
Phase 9, which deferred it to Phase 12. It does not say how the engine is given the server's key,
what "idle" means for a server that jobs share, or what happens when another model is chosen.
Decision:
1. `LlmHost` starts the server with the engine's own code path, `oc_core::sidecar::server::
   OwnedServer` — port from `oc_net::loopback::free_port` on `127.0.0.1`, a fresh CSPRNG key in
   `LLAMA_API_KEY`, `-np 1`, registered with `supervise` from the moment it is spawned — and asks
   `GET /health` through `oc-net` every `llm.health_poll_millis` for up to `llm.load_timeout_secs`
   (new, provisional, 120 s; each probe `llm.health_probe_timeout_millis`, new, provisional, 1 s).
2. The key reaches an engine as a **file**, `<app data>/run/llm.key`, created fresh with mode
   `0600` on Unix — the job spec's `ai.api_key_file`, which D10 and the schema already name. Never
   argv (visible in `ps`) and never the job spec (kept on disk with the job). The run directory is
   emptied when the app starts and the key is deleted with the server.
3. "In flight" for the app's server is **a job holding a lease**, not a single HTTP call: a job's
   lease is `call_started` at acquire and `call_finished` at release, so `OwnedServer::kill_if_idle`
   stops the server `llm.idle_kill_secs` after the last job ended, never during a job however long
   its deterministic stages take. The app's supervisor clock calls it.
4. A different model restarts the server, but only when no job holds it (`LlmError::Busy`
   otherwise — unreachable with `desktop.max_concurrent_jobs = 1`).
5. `-t` is the machine's core count (`available_parallelism`, one if it cannot say). With one
   conversion at a time the engine's rayon pool and the server do not run their heavy parts at
   once; ARCHITECTURE §8.3's "set from one place" is revisited with Phase 10's measurements.
6. The server binary is `llama-server` beside the app's executable (Tauri `externalBin`, staged by
   Phase 15). Nothing starts it until AI assistance can be turned on (part B2).
Evidence: `the_app_server_listens_on_loopback_and_hands_its_key_over_a_private_file`,
`the_app_server_is_shared_by_jobs_and_stopped_when_idle` (fails with the lease accounting removed),
`another_model_restarts_the_server_only_when_no_job_holds_it`,
`the_app_server_does_not_outlive_the_app`, `a_server_that_cannot_start_is_an_error` — all against
`oc-stub-llama-server`, Linux. **Unverified here:** macOS, Windows, and a real server with a real
model (no GGUF can be fetched).
Affects: PHASE 9 detail 3, PHASE 12 Files, `thresholds.toml` (`llm.load_timeout_secs`,
`llm.health_probe_timeout_millis`), `apps/desktop/src-tauri/src/{llm.rs,fs_scope.rs,main.rs}`.

## 2026-09-23 · The app ends an engine's whole process tree · Phase 12
Context: ARCHITECTURE §8.2 and PHASE 9 detail 5: the app puts each engine in a job object
(Windows) or its own process group (Unix) and ends it with the job or `kill(-pgid)`; a `Drop` guard
alone is insufficient. Phase 9 built the engine's side (`oc_core::sidecar::supervise`) and left the
app's to Phase 12. CLAUDE.md keeps `#![forbid(unsafe_code)]` in the desktop crate.
Decision:
1. `apps/desktop/src-tauri/src/tree.rs`. **Unix:** the engine leads a new process group (it
   already did); a kill is `kill_process_group(pgid, SIGKILL)` from `rustix` (safe API). An engine
   that exits by itself has its group **swept before it is reaped**, looked at with
   `waitid(P_PID, WEXITED | WNOHANG | WNOWAIT)`: until the reap the engine's zombie holds the group
   id, so a sweep can never reach a group that reused the number. Once reaped, the tree never
   signals the group again. Every unreaped group is listed process-wide for `end_all`.
2. **Windows:** a job object per engine with `KILL_ON_JOB_CLOSE`, through `win32job` 2.0.3
   (MIT OR Apache-2.0, a safe wrapper over `windows` 0.61, which Tauri already pulls in). Ending
   the tree is closing the job; Windows closes it when the app dies however it dies. The engine is
   assigned just after spawn (no `CREATE_SUSPENDED` without FFI of our own); the engine reads its
   spec before it starts anything, so nothing escapes. The plan's `PROCESS_MEMORY` and `JOB_TIME`
   limits are resource limits, and stay with Phase 14's hardening (SECURITY §4).
3. `oc_core::sidecar::supervise::on_teardown(hook)`: `kill_all` — and so the panic hook and the
   signal handler — also run registered hooks. The app registers `tree::end_all` at start and runs
   it at `RunEvent::Exit`, before stopping its model server. A SIGINT/SIGTERM to the app now tears
   everything down and exits 3 (supervise's "a signal to stop is a cancellation").
4. **PROVISIONAL — needs maintainer ratification:** `win32job` is the "reviewed wrapper crate"
   the Phase 9 entry of the same date anticipates for job objects; nobody has reviewed it, and it
   has not run here. `tree.rs` compiles and is clippy-clean for `x86_64-pc-windows-msvc` and
   `aarch64-apple-darwin` (checked with a scratch crate that includes the file; the desktop crate
   itself cannot be cross-checked because `ring` needs the MSVC C toolchain).
Evidence: `a_killed_engine_takes_its_whole_process_tree_with_it` and
`an_engine_that_crashes_leaves_nothing_behind` (both fail — 30 s hangs, the stderr pipe held open
by the orphan — with the group kill or the sweep removed), `quitting_the_app_ends_every_running_engine`,
`a_dropped_engine_handle_ends_its_tree`, `teardown_hooks_run_with_the_registered_children`. Linux.
**Unverified here:** macOS (same code) and Windows (job objects).
Affects: ARCHITECTURE §8.2, PHASE 9 detail 5, `Cargo.toml` (`rustix`, `win32job`),
`crates/oc-core/src/sidecar/supervise.rs`, `apps/desktop/src-tauri/src/{tree.rs,engine.rs,main.rs}`.

## 2026-09-23 · The app's model manager: one store, the licence first, Cancel deletes · Phase 12
Context: PHASE 12 detail 9 and row 12.12 ("progress events arrive; cancelling deletes the
`.part`"), UI_UX §2.4 and §3, LICENSE_AND_DEPENDENCIES §6. The first-run mockup (`firstrun.html`
step 2b) shows a cancelled download keeping its partial file with "Resume download" and "Discard
partial file", and `components.md` has "failed (Retry, resumes)"; UI_UX §4 says a failed download
resumes "where the transport supports it".
Decision:
1. **Cancel deletes the `.part`**, as row 12.12 says; behaviour follows the plan, not the mockup
   (`docs/design/README.md`: the design governs appearance only). A cancelled row is simply "not
   installed" again. `oc-net`'s transport sends no `Range` request, so it does not "support" a
   resume: **Retry starts again from the first byte**, and a failed download leaves nothing on disk
   either. `DownloadProgress::cancelled()` (default `false`) is asked between chunks and ends the
   pull with `NetError::Cancelled`; a first download that fails or is cancelled also removes its
   empty model directory. A transfer stalled inside one read sees the cancel when the read returns;
   the row shows "cancelling" until then.
2. **One store for the CLI and the app**: `oc_net::store::default_root()` (moved from
   `cmd_model.rs`), `<data dir>/openconvert/models`, so a model is fetched once. The registry the
   app reads is the one the engine compiles in (`oc_net::registry::BUNDLED`, moved likewise).
3. **The licence is accepted per model and per licence**, in `<app config>/licenses.json`
   (`{model id: SPDX}`); a changed licence needs a new acceptance. The Rust side refuses a pull
   nobody accepted (`UiError::LicenseNotAccepted`), so the webview cannot skip the step. The text
   shown is `oc_net::download::license_text`, the same text written beside the model.
4. **Rows are `ModelReadiness` plus two things the manager owns**: `license_accepted` and the
   download's state (`idle` / `downloading{done,total}` / `cancelling` / `failed{kind}`). The app's
   `models::readiness` builds the rows the way `cmd_model::readiness` does; `oc-net` may not depend
   on `oc-core` (ARCHITECTURE §3.1), so the function exists twice and an engine-integration test
   (`the_app_and_the_cli_list_the_same_models`) holds the two outputs equal.
5. **A registry with placeholder pins offers no model at all**: `ModelRegistry` refuses the whole
   file, so the screen says models are not available in this build, and why. That is today's
   shipped state (Phase 9 Blocked item 1).
6. Progress is announced at most once per percent, as `openconvert model pull` does.
Evidence: `model_download_progress_streams_and_cancels` (fails with the cancel ignored),
`a_model_is_downloaded_only_after_its_license_is_accepted`, `a_failed_download_is_a_row_to_retry`,
`model_rows_are_the_readiness_fields`, `an_unpinned_registry_offers_no_download`,
`the_app_and_the_cli_list_the_same_models`, `a_cancelled_download_deletes_its_part` (oc-net). All
against `oc-testkit`'s loopback model host through the real client and allowlist. **Unverified
here:** a download from huggingface.co (egress 403, and no pins).
Affects: PHASE 12 detail 9, row 12.12, UI_UX §2.4/§4, `docs/design/handoff/firstrun.html` step 2b,
`crates/oc-net/src/{download.rs,verify.rs,store.rs,registry.rs,lib.rs}`,
`crates/openconvert/src/cmd_model.rs`, `apps/desktop/src-tauri/src/{models.rs,main.rs}`.

## 2026-09-23 · Packs: the model mechanism over a pack registry; the validation pack not yet available · Phase 12
Context: PHASE 12 detail 9: "the same mechanism installs the optional validation pack (jlink'd
minimal JRE + `epubcheck.jar`, ~40–50 MB) and later the OCR pack — one download mechanism, three
payloads, all SHA-256 pinned". LICENSE_AND_DEPENDENCIES §6 says the runtime's licence must be
verified per vendor before the pack ships. No document says where the pack is built or hosted,
and nothing lets a conversion use an installed EPUBCheck (the job spec has no field for it; the
engine's Tier 2 is `--tier 2` with `java` and a jar found the CI way).
Decision:
1. `oc_net::download::Artifact` is what the downloader fetches; a model entry and a pack entry
   both become one (`pull` is `pull_artifact` of the model's). `oc_net::packs` parses `packs.toml`
   (compiled in, like `models.toml`) with the model registry's pin rules plus placeholders refused
   in `license`, `repo` and `file`. Packs live in `<data dir>/openconvert/packs/<id>/`.
2. The app's manager is generic over a `Catalog` (`models::Manager<C>`): `ModelManager` over the
   model registry, `PackManager` over the pack registry — the same licence-first rule, progress
   events (`pack-changed`), Cancel and store. A pack row is `PackReadiness` (id, name, contents,
   installed, size, licence, licence path), read from the registry and the store.
3. **PROVISIONAL — needs maintainer ratification:** `packs.toml` ships with the validation pack's
   `license`, `repo`, `revision`, `file` and `sha256` as `TODO_` placeholders, so the Packs screen
   says it is not available in this version. Before it can be filled: choose the Java runtime's
   vendor and verify its licence, bundle that licence's text with EPUBCheck's BSD-3-Clause in
   `oc-net` (the downloader writes only licences whose text it carries), build and publish the pack
   on a host the allowlist names (today only the model host), and decide how a conversion is told
   to use it (a job-spec field and the engine's Tier-2 runner reading the pack's runtime and jar).
   The pack is installed as one verified file; unpacking it belongs with that engine step.
   `xtask ci-lint --release-branch` does not refuse `TODO_` in `packs.toml`: v1 can ship without
   the optional pack, as it ships without the OCR pack.
Evidence: `pack_registry_rejects_placeholders`, `a_pack_installs_through_the_model_downloader`
(oc-net), `the_validation_pack_installs_through_the_model_mechanism` (licence refused then
accepted, progress streamed, `LICENSE` beside it), `the_shipped_validation_pack_is_not_available_in_this_version`.
Affects: PHASE 12 detail 9, D6, LICENSE_AND_DEPENDENCIES §6, `packs.toml`,
`crates/oc-net/src/{packs.rs,download.rs,store.rs,registry.rs}`,
`apps/desktop/src-tauri/src/{packs.rs,models.rs,main.rs}`.

## 2026-09-23 · The Models, Packs and first-run screens render the manager's rows · Phase 12
Context: PHASE 12 detail 9 ("the models screen renders exactly the `ModelReadiness` fields") and
detail 2 of "Visual design" ("every number shown comes from an engine event, the report,
`ModelReadiness` or `thresholds.toml`"); screen-map `models` and `firstrun`; the design strings'
first-run costs ("~1,100 MB download", "~1–2 GB RAM") are mockup placeholders. Two registry fields
are English prose: a model's `warn` and a pack's `contents`.
Decision:
1. `ModelRow.svelte` (components.md `ModelRow`/`PackRow`) renders a row's fields and nothing else:
   size and RAM estimate as the locale writes bytes (binary units, as the cache and memory cap are
   counted), the CPU expectation through UI_UX §2's four words in the user's language (any other
   string as the registry has it), the licence and, once installed, its path. Download progress is
   the bytes the manager reports (`role="progressbar"`, bytes as `aria-valuetext`).
2. The first-run costs are `firstrun.cost.download`/`firstrun.cost.ram` with a `{size}` slot, filled
   from the default model's `size_bytes` and `ram_estimate_bytes`; the design's figures are gone
   from all three locales. The card shows only while the queue is empty, the registry is usable,
   the default model is not installed and "Not now" was never pressed (design decision 10).
   "Set up AI assistance" opens route `firstrun`: the Models section with only the default row, its
   licence shown, and "Back to the queue" once it is installed.
3. A model's `warn` and a pack's `contents` are registry prose in English. The UI shows a sentence
   in the user's language instead — `models.warn` whenever `warn` is present, and
   `packs.contents.<id>` for a pack it knows — never the English text (A12.3's "never an English
   fallback"). The registry's words stay for maintainers and `openconvert model list`.
4. The AI toggle stays disabled, and its help now says why: the converter has no AI support in this
   build (part B2). The drafts of every new DE/TR string need the same native review as the rest.
Evidence: `renders exactly the ModelReadiness fields of each row`,
`model_download_progress_streams_and_cancels — the licence first, real bytes, a working Cancel`,
`an installed model offers Delete; a failed download offers Retry`,
`a registry without pins offers no model, and the validation pack is not available`,
`the card's costs are the default model's row; Set up opens it with its licence shown`,
`Not now hides the card and it stays hidden` (Vitest); `first_run_downloads_the_default_model_by_keyboard`
and axe/contrast on the `models` and `firstrun` screens under the shipped CSP (Playwright, Chromium).
Affects: PHASE 12 detail 9 and "Visual design" 2–3, `docs/design/handoff/strings/*.json`
(`firstrun.cost.*`), `apps/desktop/ui/src/{components/ModelRow.svelte,components/FirstRunCard.svelte,
routes/settings/Settings.svelte,routes/queue/Queue.svelte,App.svelte,lib/catalog.svelte.ts}`.

## 2026-09-23 · The verse band has one definition, and the classifier reads it · Phase 10
Context: the open finding of 2026-09-22 — `oc-structure::quotes::classify_indented` compared the
`f32` short-line ratio widened to `f64` against the band's bounds, so a block exactly on
`verse.short_line_ratio_min` (7 short lines of 20) was a block quotation to the classifier and an
escalation to `oc_core::escalation::verse_quote`.
Decision: the band's edges are defined once, `oc_core::escalation::line_band` (`Full`, `Between`,
`Short`, compared in `f32`), and `verse_quote` is written over it. The classifier asks
`verse_quote` whether a block is ambiguous and `line_band` which side of the band a settled block is
on; it passes `blocks_remaining: u32::MAX`, because the block budget is the AI step's to spend and
not a property of a block. Behaviour is unchanged everywhere except on the lower bound itself.
Evidence: `quotes::tests::a_block_exactly_on_the_lower_bound_is_ambiguous_not_a_quotation` fails on
the widening (`left: BlockQuote, right: Ambiguous`) and passes on the fix; the ten fixtures'
`--no-ai` EPUB hashes, pinned before the change in `ai__no_ai_epub_sha256.snap`, did not move.
Affects: `oc-core::escalation`, `oc-structure::quotes` (the finding is closed).

## 2026-09-23 · Task 2: what a heading mapping may change, and what the pre-gate cannot check · Phase 10
Context: PHASE 10 detail 3 and ARCHITECTURE §9.6 say what the heading-roles call is shown and how
its answer is checked; they do not say what an admitted role *does* to a book, and two of the
checks they name have no input in this codebase.
Decisions, each **PROVISIONAL — needs maintainer ratification**:
1. **Only size-rank levels are touched.** A heading whose level the outline or the printed contents
   page bound, or a numbering pattern refined, keeps it: size rank is the fallback the task stands
   in for (ARCHITECTURE §6.1). The Typst fixtures all carry outlines, which is why test 10.8 clears
   the outline first — without that the property held vacuously, and a mutation that emptied every
   demoted block passed (checked).
2. **Roles to levels:** part 1; chapter 1, or 2 when a heading cluster is a part; section chapter+1;
   subsection chapter+2; the skip repair runs again afterwards. `body` demotes a heading to a
   paragraph and `epigraph` to an epigraph wrapper. **`other`, `caption` and `running_head` change
   nothing**: `other` is the prompt's own abstention (Appendix A.1 rule 3), and `running_head` is a
   proposal furniture has already declined (D13.5). The edit type has no removal variant.
3. **One rule beyond ARCHITECTURE's two:** a mapping that demotes every cluster holding a heading
   is refused (`S.roles`). One answer should not be able to take a book's whole navigation away.
4. **The silhouette floor is not checked.** Phase 4's clustering computes no silhouette and
   `thresholds.toml` has no floor, so the pre-gate is the two measured conditions.
5. **Fewer than `inventory.holdout_min_probes` (8) held-out lines → no call** (`pregate.holdout`):
   "send 8–10" read as a minimum, since an unchecked mapping is not one to ask for.
6. **Run-in candidates do not ride along.** PIPELINE §8.2 has them in the heading-roles call, but
   the frozen v1 payload has no slot for them; that needs a v2 prompt.
New thresholds: `inventory.holdout_{min,max}_probes` (8, 10 — the max held equal to the grammar's
`"h"` bound by a test), `inventory.chapter_cluster_{min,max}_count` (2, 200).
Evidence: rows 10.5–10.8 and `role_rules_refuse_what_the_design_forbids` in `crates/oc-ai/tests/tasks.rs`;
`running_head_label_never_deletes_text` in `crates/openconvert/tests/ai.rs`.
Affects: ARCHITECTURE §9.6 task 2, PIPELINE §8.2, `oc-ai::task::heading_roles`,
`oc-structure::headings::levels`, `oc-structure::stage::structure_with`.

## 2026-09-23 · Task 3: strictly increasing boundaries, local chunk indices, a partial answer · Phase 10
Context: test 10.9 asserts `frontmatter_end_idx >= part_boundaries[0]` is rejected, and ARCHITECTURE
§9.6 says "all indices strictly increasing". The frozen v1 prompt defines the front boundary as
"the index of the first heading after the front matter (0 if there is none)" — an *exclusive* end.
Under that definition a book whose body opens with a part answers `front == parts[0]` correctly.
Decisions:
1. **PROVISIONAL — needs maintainer ratification:** the rule is strict, as the test and
   ARCHITECTURE state it, equality included. A book whose body opens with a part has its correct
   answer refused (`S.order`) and keeps the deterministic structure — a lost improvement, never a
   wrong edit. Two ways out, both outside this phase: ratify `front ≤ parts[0]`, or a v2 prompt
   whose front boundary is the last front-matter heading.
2. **Each chunk is its own question with indices from zero**, as the prompt's "0 if there is
   none" and "the number of headings if there is none" read; the answer is mapped back to global
   indices before stitching. Global indices in a middle chunk would make both conventions
   ambiguous.
3. **Stitching compares per-heading places** — zone and part flag — over every heading two chunks
   both saw; one difference rejects the whole answer (`S.overlap`, test 10.10). The stitched zones
   must still run front, body, back (`S.order`).
4. **A chunk the call budget did not grant is not asked**, and the headings only it covered keep
   the deterministic zones (the degradation order drops "`book_structure` chunks beyond the first",
   D13.6). The first chunk's answer is still applied to the headings it saw.
5. **The edit is a zone and a part flag per heading** (`oc_structure::book::ZoneEdits`), applied by
   `book_structure_with` on the same headings at the same levels; a heading the numbering reads as
   `Part` stays a part whatever the label says.
New thresholds: `llm.book_structure_chunk_headings` (200, the design's) and
`llm.book_structure_chunk_overlap` (20, invented).
Evidence: rows 10.9–10.11, `agreeing_chunks_stitch_and_ungranted_chunks_are_not_asked`,
`zone_labels_place_the_headings_they_cover`.
Affects: ARCHITECTURE §9.6 task 3, PIPELINE §9, `oc-ai::task::book_structure`, `oc-structure::book`.

## 2026-09-23 · The language gate ships empty; an unproven task runs only under `--ai-all-tasks` · Phase 10
Context: PHASE 10 detail 7 — a task ships "enabled-by-opt-in" only when it is non-inferior to the
deterministic path with a false-repair rate ≤ 1 % in every category, per language, and "otherwise
it stays behind a flag"; the gating map lives in `thresholds.toml` as `[ai.task.<task>.languages]`.
No evaluation can run here: no model is reachable (huggingface.co refused).
Decisions, each **PROVISIONAL — needs maintainer ratification**:
1. **All four maps ship empty.** No task has passed, so none is enabled for any language: `--ai`
   alone asks nothing, and every escalation it would have sent is recorded with
   `fallback = "language.gate"`. The alternative — enabling all three languages unmeasured — would
   make the opt-in an unmeasured one, which detail 7 rules out.
2. **The flag is `--ai-all-tasks`**: with `--ai`, it sets the language gate aside and runs every
   escalated task for every language. It is how the evaluation's deterministic+LLM arm runs, and
   how a maintainer experiments; the report records that it was set. It is not in §2.1's flag list,
   which predates detail 7's "behind a flag".
3. **`thresholds.toml` gains array values**: a closed set of names is a decision with provenance
   like any number, so `oc-core`'s build script emits a string array as `&'static [&'static str]`.
   Everything else still has to be a float, an integer or a boolean.
4. **The wall-clock hard stop raises `W_LLM_TIME_EXHAUSTED`**, not `W_LLM_BUDGET_EXHAUSTED` as
   detail 6 writes: that template says "all {calls} of its model calls", and a warning is a factual
   claim (R10 §6.20). Both are "budget exhausted"; the report says which budget.
5. **The degradation order drops, after "chunks beyond the first" and `heading_roles`, the first
   book-structure chunk** — the order D13.6 gives ends there, and metadata is never dropped for
   another task. With no call left at all, nothing is asked, metadata included, and the budget
   says so.
6. **The session stops at the first unreachable provider** (`llm.unavailable`): a dead endpoint is
   asked once per book, not eight times, and the book is deterministic from there with
   `W_LLM_UNAVAILABLE` (RT D20).
Evidence: `crates/oc-ai/tests/plan.rs` (rows 10.21, 10.22 and three session tests).
Affects: PHASE 10 details 5–7, IMPLEMENTATION_PLAN §2.1, `thresholds.toml`, `oc-core` build script,
`oc-ai::{plan, session}`.

## 2026-09-23 · The AI step: where it runs, how an edit reaches the book, what is recorded · Phase 10
Context: PHASE 10's file list puts the step in `crates/oc-core/src/stages/ai.rs`. `oc-core` sits
below every stage crate (they read `T` from it) and cannot depend on `oc-structure`, and it may not
reach `oc-ai` either without `oc-structure` reaching it through `oc-core` (ARCHITECTURE §3.1:
`oc-structure` must not depend on `oc-ai`). The stage driver has lived in `openconvert` since
Phase 5 (2026-09-20, "the benchmarks live in `openconvert`").
Decisions:
1. **The step is `openconvert::ai`**, and `openconvert` gains the `oc-ai` edge — a workspace crate
   with no network dependency, already reached through `oc-net`. `oc-core` gains nothing: the
   escalation predicates were already there (Phase 8), and the stage set is unchanged — the step
   runs inside `structure`'s slot and is timed as `ai` only when it runs.
2. **An admitted answer is applied by re-running `structure`** with every edit admitted so far
   plus the new one (`structure_with`), and gates L and V compare that run with the previous one.
   The final run is checked under the conservation law exactly as the deterministic one is. Nothing
   patches output; a model's label reaches the book through the code the rule's label took.
3. **Order:** metadata, heading roles, verse or quote, then book structure over the heading list
   the earlier edits left. Book structure's `Decision` names the `document` stage (PIPELINE §0.4),
   although its zones are applied by `oc_structure::book`, where the section tree is built.
4. **PROVISIONAL — needs maintainer ratification: heading roles is not asked when no heading has a
   size-rank level** (`pregate.headings`). The edit touches only size-rank levels (2026-09-23,
   task 2), so an answer could change nothing: gate D, "if deterministic evidence is sufficient,
   the model is never consulted". The predicate in `oc_core::escalation` is unchanged; this is a
   pre-gate, like the inventory's.
5. **A verse label its counter-evidence overrode** is recorded with the model's trace and
   `fallback = "counter_evidence"`: the model was asked and the rule's answer stood.
6. **The report** gains `ai` (model id, `--ai-all-tasks`, calls, cached calls, LLM milliseconds),
   omitted with AI off, and `engine.prompt_version` is set when the step ran.
Evidence: `crates/openconvert/tests/ai_pipeline.rs` — rows 10.14, 10.15, 10.20, A10.2, and 10.2
with AI on. With a cooperative in-process model all four tasks are applied somewhere across the
twenty fixture variants and I-7 holds on every one.
Affects: PHASE 10 file list, ARCHITECTURE §3.1 (`openconvert → oc-ai`), PIPELINE §0.4, the report.

## 2026-09-23 · `convert --ai`: the flags, the refusal, and where answers are cached · Phase 10
Context: §2.1 lists `--ai`, `--no-ai`, `--llm-endpoint`, `--llm-api-key-file` and `--model-path`;
PHASE 9 left their arrival to Phase 10 ("a flag that does nothing is worse than no flag").
Decisions:
1. **`--no-ai` wins over `--ai`**, and the AI-only flags (`--ai-all-tasks`, `--llm-endpoint`,
   `--llm-api-key-file`, `--model-path`) without `--ai` are a usage error, not ignored.
2. **PROVISIONAL — needs maintainer ratification: an endpoint that is not this machine is refused
   (exit 2) until Phase 11.** D10 requires consent before a book's text leaves the machine; the job
   spec has `non_loopback_consent` and the command line has nothing yet. Phase 11 (A11.2) adds it.
3. **Every other failure to reach a model converts deterministically with `W_LLM_UNAVAILABLE`**
   and a reason (no `llama-server`, no installed model, a server that never became ready, an
   endpoint that did not answer) — exit 0 (RT D20). With the shipped, empty language maps nothing
   is asked, so a dead endpoint is only noticed under `--ai-all-tasks`; the banner is not raised for
   a call that was never made.
4. **The engine-owned server** is `OC_LLAMA_SERVER`, else a `llama-server` beside the engine (the
   desktop bundle's `externalBin`, D8); the model is `--model-path` or the registry default in the
   model store. It is started per conversion and stopped when the conversion ends. Four thresholds:
   `llm.{load_timeout_secs, call_timeout_secs, health_probe_timeout_millis, sidecar_context_tokens}`.
5. **The cache lives in the data directory** (`<data>/openconvert/cache/llm`, ARCHITECTURE §9.4),
   which `openconvert::data_dir` now defines once for the model store and the cache alike. An
   external endpoint's model id is `--model-path`'s file stem when given, else `endpoint@<host>` —
   the cache cannot know which model sits behind someone else's server; Phase 11 names providers.
6. **`llm` NDJSON events** are emitted after the conversion, one per call, cached ones included.
Evidence: `crates/openconvert/tests/ai_cli.rs` (rows 10.16, 10.19 and two more);
`ai_endpoint::only_this_machine_is_loopback`.
Affects: IMPLEMENTATION_PLAN §2.1, D10, Phase 11, `openconvert::{ai_endpoint, data_dir}`, `cmd_convert`.

## 2026-09-23 · The AI evaluation: what is built, what is seeded, what cannot run here · Phase 10
Context: PHASE 10 detail 7 and rows 10.17/10.18 — run the corpus twice per task, tabulate the paired
2×2 per assertion category and language, McNemar (χ², exact below 25), the false-repair rate
`c / n` per category, and gate every enabled task at ≤ 1 %. No model is reachable here and the
corpus is not downloaded.
Decisions:
1. **`eval/src/oc_eval/compare/`** holds the statistics (`mcnemar`, reusing G7's exact test and
   non-inferiority), the table and gate (`false_repair`), the gold format (`gold`), the scoring of
   gold items against both paths' answers (`score`) and the document (`render`).
   `python -m oc_eval.compare --render | --check | --gate`; CI's eval job runs `--check` and `--gate`.
2. **Standard library, not scipy**, for both McNemar forms (`math.comb`, `math.erfc`), as Phase 9's
   G7 did: the plan names scipy, and it stays a declared dependency, but an untyped import would cost
   the mypy gate for two closed-form functions.
3. **The assertion category is the gold item's `category`** — the metadata field, the heading role,
   the zone, the block kind. Phase 7's `.assert.json` vocabulary has no per-task AI categories; a
   gold item is one assertion.
4. **The four gold sets are seeds read off the Typst fixtures' sources** (`ours(typst)`, 47 items).
   They fix the format; D18 forbids fitting or deciding on `ours(*)` alone, and the document shows
   each set against `calibration.min_gold_instances_per_task` (200).
5. **With nothing enabled, row 10.18's gate passes vacuously and says so**; an enabled task with no
   outcomes fails ("a gate that did not run is never a pass", Phase 9). `docs/AI_EVALUATION.md`
   records that no evaluation has run. **Unverified here:** A10.4 and A10.5 — every measured number.
6. `oc_eval.model_gate.fixtures` now picks each task's **seed** cassette by its index name
   (`<task>__a3__v1`): Phase 10 recorded more cassettes beside the seeds, and the loader assumed one.
Evidence: `eval/tests/test_ai_evaluation.py` (rows 10.17, 10.18 and four more).
Affects: PHASE 10 detail 7, D18, `thresholds.toml` (`ai_eval.*`), CI's eval job, `docs/AI_EVALUATION.md`.

## 2026-09-23 · VD-g closed: the UB-Mannheim installer's paths and version string · Phase 13
Context: VD-g (Phase 0 verification-debt table; TECHNOLOGY_EVALUATION §10 / V2 §7) blocks Phase 13's
system-Tesseract discovery: the Windows probe has to look where the UB-Mannheim installer actually
puts `tesseract.exe`, for the version it actually delivers.
Decision: the Windows well-known list is exactly the plan's two entries, in this order —
`%ProgramFiles%\Tesseract-OCR\tesseract.exe` (label `program-files`), then
`%LOCALAPPDATA%\Programs\Tesseract-OCR\tesseract.exe` (label `local-app-data`) — and the version
parser accepts the UB-Mannheim banner form `tesseract v5.x.y.YYYYMMDD` alongside `tesseract 5.x.y`.
Evidence, each read 2026-09-23:
1. **Install path.** The installer script, `nsis/tesseract.nsi` on the `main` branch of
   `github.com/UB-Mannheim/tesseract`: `!define PRODUCT_NAME "Tesseract-OCR"`,
   `!define MULTIUSER_INSTALLMODE_INSTDIR ${PRODUCT_NAME}`, `!define MULTIUSER_USE_PROGRAMFILES64`,
   `!include MultiUser.nsh`, installed files under `$INSTDIR` with `tessdata\` beside the binary.
   NSIS's own `Contrib/MultiUser/MultiUser.nsh` (`github.com/NSIS-Dev/nsis`, `master`) sets the
   all-users `$INSTDIR` to `$PROGRAMFILES64\${MULTIUSER_INSTALLMODE_INSTDIR}` and the current-user
   one to `GetKnownFolderPath {5CD7AEE2-2219-4A67-B85D-6C9CE15660CB}` (FOLDERID_UserProgramFiles,
   i.e. `%LOCALAPPDATA%\Programs`) + `\Tesseract-OCR`. So the two install modes land exactly where
   the plan's list looks. The installer writes `HKLM\…\Tesseract-OCR` `Path`/`InstallDir` registry
   values and **does not modify `PATH`** (no `EnvVarUpdate` or equivalent in the script), which is
   why on Windows the well-known list, not `PATH`, is the usual way it is found. Reading the
   registry value would find a custom install directory; it needs a Windows API binding and is
   left out of v1 (a custom directory is reachable with `--ocr-path`).
2. **Version.** The UB-Mannheim wiki (`github.com/UB-Mannheim/tesseract/wiki`) lists the latest
   installer as `tesseract-ocr-w64-setup-5.5.3.20260724.exe`, 64-bit only — Tesseract 5.5.3, well
   above the ≥ 5 floor. Its builds print their version with a `v` and the build date: tesseract
   issue #4034 quotes `tesseract v5.3.0.20221214` / `leptonica-1.78.0` from one. A parser written
   against Linux's `tesseract 5.3.4` alone would have refused every Windows install as unreadable;
   `every_platforms_version_banner_parses` holds both forms.
3. **Language data.** English is a mandatory installer section and every other language is an
   optional one (`SectionIn RO` for English, `/o` for the rest, each downloaded during install), so
   a Windows user who did not tick German or Turkish has `eng` only. That is the case
   `W_OCR_LANG_MISSING` exists for, and its Windows hint names the installer's option.
Not verifiable here: that a real UB-Mannheim install on a Windows machine is found by the probe.
There is no Windows machine and no CI runner; the list is asserted as data
(`the_well_known_lists_are_the_documented_ones`) and the Windows row of A13.1/A13.2 is unverified
here.
Affects: IMPLEMENTATION_PLAN Phase 0 VD-g (closed), PHASE 13 detail 1, `oc_core::ocr::discover`.

## 2026-09-23 · OCR's ledger entries carry their region; a whole page is read in clean bands · Phase 13
Context: PHASE 13 detail 7 wants one `Ocr` entry per region "carrying the region bbox", and I-6
(ratified note N-1) checks that region for pre-existing text. IR_SKETCH's `LedgerEntry` has no
geometry. And an `ImageOnly` page is `visible_chars < pageclass.image_only_max_visible_chars`, not
zero: a scan with a stamped folio or a producer's watermark line in real PDF text is still
`ImageOnly`, so a whole-page region would contain pre-existing text and fail I-6.
Decision:
1. `LedgerEntry` gains `region: Option<Rect>`, set only by OCR and not serialised when `None`, so
   every existing ledger, snapshot and report reads exactly as before. Additive, like Phase 8's
   `Decision.fallback`. `LedgerEntry` and `LedgerDelta` lose `Eq` (a `Rect` is `f32`); nothing
   compared them with more than `PartialEq`.
2. I-6 is its own function, `oc_core::ledger_check::check_i6`, because it needs the page's runs as
   well as the ledger. "Contains a run" is read as *overlaps with area*: the stricter reading, since
   a region that overlapped PDF text would put OCR's copy of that text beside the PDF's own. An
   `Ocr` entry that removes, or has no region, is also an I-6 failure.
3. A whole-page OCR region is cut into full-width horizontal bands that avoid every pre-existing run
   (`clean_bands`); each band with words is one entry. A word that straddles a cut is dropped — it
   is on the line of text the PDF already carries. On a clean scan this is one band, the page.
4. OCR-added characters are counted in `ReasonTotals::ocr_added` and `I7Result::ocr_chars` and are
   taken out of the retention **numerator**; `C_0`, the denominator, never contains them (RT C1).
   `ocr_chars` is omitted from the report when zero so born-digital snapshots do not change.
Evidence: `i6_region_scope_rejects_overlapping_text`, `a_full_page_region_is_cut_into_bands_around_existing_text`,
`ocr_regions_excluded_from_source_retention`, `retention_excludes_ocr_added_characters`.
Affects: IR_SKETCH `LedgerEntry` (additive field), ARCHITECTURE §5.4 I-6, PHASE 13 details 7 and 8.

## 2026-09-23 · OCR in `ingest`: where it runs, what it replaces, and what `ingest` now records · Phase 13
Context: PHASE 13 details 3, 7–11 route OCR by page class inside `ingest` and merge its runs there.
This codebase's `ingest` is `openconvert::input::page_inputs` (glyphs, fonts, images per page); the
plan's `crates/oc-core/src/stages/ingest.rs` routing cannot live in `oc-core`, which `oc-pdf`
depends on, so it cannot see a `PdfDoc`.
Decision:
1. **Routing is `openconvert::ocr::ocr_stage`**, called inside `convert`'s `ingest` timing, after
   extraction and before `text`. `oc-core` keeps the engine adapter (`ocr::{discover, invoke, tsv,
   lang, merge}`), the `INGEST` declaration (`stages/ingest.rs`) and I-6 (`check_i6`). OCR runs ride
   on `PageInput.ocr_runs`; `text` appends them after the runs it assembles from glyphs.
2. **`ingest` is now checked and recorded** — I-1, I-2 and I-6 over the OCR step — and its
   `StageCheck` is the first entry of `per_stage_checks`. Before this phase `ingest` added nothing
   and was not in the ledger at all. So a born-digital book's ledger gains one Conserving-looking
   `ingest` entry (0 removed, 0 added), the report's per-stage list has nine entries, and the f07
   report snapshot changed for that reason alone. The glyph filters' own removals (`GeneratedSpace`,
   `HiddenText`, …) are still not pushed into the document ledger; that predates this phase and
   `C_raw` bookkeeping is Phase 7.5's to settle.
3. **The picture OCR replaced leaves the book** when the region's mean confidence is at or over
   `ocr.region_conf_min`; under it, the picture stays beside the text with `W_OCR_LOW_CONFIDENCE`
   (detail 9). A region that yields no words, or whose call fails or hangs, keeps its picture
   (`W_OCR_FAILED` for the latter), and the book completes.
4. **Image ids are numbered page-locally at extraction** (`input::number_images`), and decoding
   reads that id (`structure_input::image_slots`). The old code recovered the page-local index from
   positions in the document-wide list, which is only right while no image leaves the list; OCR
   removing one would have made every later image on its page decode as its predecessor. Output is
   unchanged for every existing fixture (the snapshot suite is green without edits for it).
5. **I-7's `Removed_all` leaves out the two dedup reasons** (`OverdrawDedup`, `OcrLayerDuplicate`,
   `Reason::folded_into_c0`). ARCHITECTURE §5.2 folds dedup into `C_0` "rather than recorded against
   `C_raw`", so the baseline already lacks those characters; counting them again made a re-OCR'd
   sandwich fail I-7 by exactly its old layer. The entries stay in the ledger as the record.
   *Found, not fixed:* the same double count applies to `text`'s `SoftHyphen`/`LigatureExpand`
   entries (`C_0` is taken after `N`), which would fail I-7 on any book with a soft hyphen or a
   ligature code point. No fixture has one, and it is Phase 7.5's class, not OCR's.
6. **A re-OCR's `OcrLayerDuplicate` removal is not charged to `conservation.budget.ocr_layer_duplicate_per_page`.**
   PIPELINE §3 defers `ingest`'s budget checks to `text`, and the existing code never charged any
   `ingest` removal; re-OCR replaces 100 % of a page's layer by design, which the 0.60 per-page
   dedup allowance would forbid outright. No budget was widened. **PROVISIONAL — needs maintainer
   ratification:** whether re-OCR's coupled removal should have a budget of its own.
7. **`convert` now emits `hello`**, with `ocr:tesseract-<version>` in `capabilities` when discovery
   found a usable engine (detail 1). `convert` emitted no `hello` before, against §2.3's "always the
   first line"; `inspect` and `dump-stage` are unchanged.
8. The rasters go into `<output>.oc-tmp-<token>/`, beside the output, removed after every call and
   the directory after the conversion.
Evidence: `ocr_e2e` (8 tests), `repair::the_ledger_records_validate_and_repair_as_conserving_stages`,
`report::the_report_carries_every_part_the_plan_names`.
Affects: PIPELINE §3, ARCHITECTURE §5.2/§5.4, PHASE 13 details 1, 3, 7–11, §2.1 (four new flags).

## 2026-09-23 · `BrokenText` pages are not OCR'd in v1 · Phase 13 · PROVISIONAL
Context: D13.10 routes `broken-text` → OCR, and PIPELINE §3's table says "OCR the whole page; the
broken text layer is removed under `HiddenText` if invisible, else kept and flagged". A broken
layer that is invisible has already been removed as `HiddenText` by extraction (render mode 3 on a
non-sandwich page), so a page that still classifies `BrokenText` has a **visible** broken layer. A
whole-page `Ocr` region over it contains pre-existing text runs, which I-6 (ratified N-1) forbids;
keeping both would put the page in the book twice; and removing visible text for being unreadable
has no reason in the closed `Reason` enum (`HiddenText` is "rendered but not visible", which this is
not). The task rules forbid adding a `Reason`.
Decision (the most conservative reading consistent with DECISIONS.md): `BrokenText` pages keep their
extracted text and the existing `W_BROKEN_TEXT_PAGES` flag, and OCR does not read them. `ImageOnly`,
`Mixed` and `--re-ocr` sandwich pages are read as the plan says.
**PROVISIONAL — needs maintainer ratification:** one of (a) a `Reason` for "replaced by OCR" (an
`ir_version` question), (b) extending `OcrLayerDuplicate` to a broken visible layer, or (c) ratifying
that v1 does not OCR broken-text pages.
Affects: D13.10, PIPELINE §3, PHASE 13 details 3 and 6.

## 2026-09-23 · Scanned fixtures, their ground truth, and three things real Tesseract taught · Phase 13
Context: PHASE 13 detail 12 and rows 13.20/13.21. Tesseract 5.3.4 (`eng`, `deu`, `tur`, `osd`) is
installed on this machine, so the real engine ran here.
Decision:
1. **Four synthetic scans** — `f01` at 300 and 200 dpi, `f04` (de) and `f05` (tr) at 300 — made by
   `python -m oc_eval.generate.scan_sim --scanned-fixtures`: pypdfium2 render in grayscale, a
   rotation drawn from ±1.5°, a 12 % left-to-right brightness gradient, Gaussian noise σ 6, JPEG
   quality 60, wrapped by img2pdf's *internal* engine at the source's page size (the pikepdf engine
   writes a random `/ID` per run). Every random draw is seeded from the fixture's name.
   **Committed** as golden binaries (1.5 MB together) under `corpus/fixtures/scanned/`, per
   TEST_CORPUS §6.4's fallback: the render and the JPEG encoder are not promised to be the same bytes
   across platforms, and D1 keeps Python out of the Rust test path. `--check` regenerates in memory
   and fails on any difference; the `ocr` CI job runs it. They are `ours(Typst)` in the manifest and
   count against `corpus.ours_max_share` (D18).
2. **Ground truth is this pipeline's own text of the born-digital source**, OCR off (`<id>.gt.txt`),
   not the source PDF's raw text layer: the scan and its source then lose the same running heads and
   folios to `furniture`, so CER measures OCR and nothing else. `scanned_ground_truth_is_the_born_digital_text`
   holds the committed files equal to the live pipeline (`OC_UPDATE_SCAN_GT=1` rewrites them).
3. **`OMP_THREAD_LIMIT=1` in `tesseract`'s environment.** On this machine at load ~9 on 4 cores, a
   page Tesseract reads in 0.96 s standalone was still running at the 30 s deadline inside a run —
   OpenMP's spinning workers — and every page degraded to a picture. Tesseract's documentation
   recommends the cap when it is not alone on the machine. Environment, not argv: the argument
   vector stays detail 2's.
4. **An OCR line's size is its median word's height**, and lines within `ocr.line_size_snap_ratio`
   (0.25, provisional, new) of a region's median are snapped to it. The line box's height is wrong
   on a skewed scan (a 1.5° climb adds ~8 pt over a 300 pt measure), and raw heights split one body
   face into several clusters, so `structure` found no heading on three of the four scans. With both,
   all four headings are found at level 1.
Measured here: synthetic-scan CER **0.0000 / 0.0011 / 0.0000 / 0.0015** (f01@300, f01@200, f04, f05),
mean **0.0007** against `ocr.max_cer_synthetic = 0.03`; all twelve `.assert.json` assertions pass.
**Unverified here:** the real stratum. The manifest's `ABBYY-scanner` holdout documents are not on
this machine (`corpus/downloads` is absent) and have no ground-truth text; the test and the nightly
report print the real stratum as `n = 0` and refuse to state a gap rather than print one against
nothing. Adding 6–10 Internet Archive volumes with hOCR-derived ground truth is the plan's
"real scans" half of detail 12 and is left for the maintainer (their download needs a rights check
per item, TEST_CORPUS §2).
Evidence: `ocr_tesseract` (feature `tesseract`), `ocr_scanned`, `test_metrics`/`test_run` additions.
Affects: PHASE 13 detail 12, TEST_CORPUS §6.3/§6.4, `thresholds.toml` (`ocr.line_size_snap_ratio`).

## 2026-09-23 · Consent lives in `oc-net`; off this machine means `https://` · Phase 11
Context: PHASE 11 detail 4 and D10 — a non-loopback endpoint needs an explicit toggle that names the
host; SECURITY §8 puts the host allowlist in `oc-net`, "not left to each provider implementation".
Decisions:
1. **`oc_net::consent::authorize(url, consent)` is the one check**, and `HttpTransport` runs it in
   its constructor: `HttpTransport::new` reaches this machine only; `HttpTransport::with_consent`
   reaches the one host a `ConsentRecord` names. A refusal happens before any socket exists, so "no
   bytes sent" is a property of the type, not of each caller remembering.
2. **Loopback** is `localhost` (exactly), 127/8, `::1`, and `::ffff:127.x` — what `std::net` calls
   loopback. `localhost.` and `*.localhost` need consent: the resolver is not obliged to agree.
3. **The URL is parsed narrowly**: user-info, `%`, `\`, `?`, `#`, whitespace and a second colon are
   refused, never interpreted (`http://127.0.0.1@evil.example` is a request to `evil.example`). An
   unparseable URL is never loopback.
4. **PROVISIONAL — needs maintainer ratification: plain `http://` off this machine is refused even
   with consent.** D10 and SECURITY §8 do not speak to the scheme. Consent says the named host may
   read the books' text; over plain http everyone on the path can, and an API key rides in the
   clear. The conservative reading refuses it (`NetError::PlaintextRemote`); a LAN server needs TLS
   in front of it. Loosening this is a one-line change in `authorize`.
5. **`ConsentScope` has one variant, `Run`**: the engine remembers nothing. The desktop app keeps a
   user's choice per configuration (UI_UX §2.4) and writes it into each job spec it runs.
Evidence: `crates/oc-net/tests/consent.rs` (row 11.6 and four more).
Affects: D10, SECURITY §8, `oc_net::{consent, transport}`.

## 2026-09-23 · The adapters, and a provider that constrains nothing · Phase 11
Context: PHASE 11's files put the adapters under `oc-ai/src/provider/`; detail 1 says an endpoint
with neither GBNF nor JSON Schema degrades to "a schema-in-prompt plus a strict Gate S" and records
`W_LLM_UNCONSTRAINED`.
Decisions:
1. **`oc_ai::openai` moved to `oc_ai::provider::openai_compatible`** (the one client, unchanged), and
   `provider.rs` became `provider/mod.rs`. `local_sidecar(…)` is that client with GBNF and
   `chat_template_kwargs`; `custom_endpoint(…)` is it with the probed `ProviderCaps` and D10's
   generic thinking lever — `/no_think` when the model id contains `qwen` (case-insensitive), nothing
   otherwise. `ProviderKind { LocalSidecar, OpenAiCompatible, Ollama }` names the adapter, not who
   owns the server: a user's own `llama-server` the probe recognises is a `LocalSidecar`.
2. **Schema-in-prompt is the task's own `schema.json`, appended after a blank line to the user
   message**, with no words around it. The shared system prefix already says "answer only with JSON
   that matches the grammar you were given", and ratified R-7 keeps prompt text out of Rust: a
   sentence introducing the schema would be prompt text in code, or a new artifact under the frozen
   v1. The cache key does not change (it hashes `request.user`, the question), which is right: the
   question is the same, and gate S judges whatever comes back.
3. **`W_LLM_UNCONSTRAINED` is raised by the `Session`, once per book, on the first answer an
   unconstrained provider gives** — not at open, because a book that asks nothing was not affected.
   Argument: `model`. Gate S is not relaxed in any way.
Evidence: `crates/oc-ai/tests/providers.rs` (row 11.4 and three more).
Affects: PHASE 11 detail 1, D10, `oc_ai::{provider, session}`, the warning registry.

## 2026-09-23 · Ollama speaks `/api/chat`, not `/v1/chat/completions` · Phase 11
Context: PHASE 11 detail 1 says every provider speaks OpenAI-compatible `/v1/chat/completions`;
detail 2 and D10 say Ollama's answer is constrained through its `format` field with the full JSON
Schema and its `num_ctx` is explicitly overridden, because the 2 048-token default truncates.
Read on 2026-09-23: Ollama's OpenAI-compatibility request type (`openai/openai.go`,
`ChatCompletionRequest`) has no `options`, `num_ctx`, `format`, `keep_alive` or `think` field, and Go's
JSON decoder drops unknown fields silently; `server/routes.go` truncates native chat messages that
exceed `NumCtx`. The two details cannot both hold.
Decisions:
1. **PROVISIONAL — needs maintainer ratification: D10 wins over detail 1.** `oc_ai::provider::ollama`
   posts to `/api/chat` — the same two messages, greedy decoding and answer as every adapter — with
   `stream: false`, `format` (the task's `schema.json`), `options {temperature, num_ctx,
   num_predict}`, `keep_alive`, `think: false`, `truncate: false` and `shift: false` (`api/types.go`
   `ChatRequest` has both: an Ollama that knows them errors instead of truncating the prompt or
   shifting it out of the context). A `num_ctx` sent to `/v1` would be ignored, and the failure it
   exists to prevent — a silently cut prompt answered well-formed and wrong — would be back.
2. **`num_ctx = max(llm.ollama_num_ctx, prompt bytes + llm.ollama_template_overhead_tokens +
   max_tokens)`.** Bytes bound tokens from above for byte-level tokenizers, so the context is never
   short; a book whose prompts fit asks for one context throughout, so Ollama does not reload the
   model between calls. Thresholds, all provisional: `llm.ollama_num_ctx = 8192`,
   `llm.ollama_template_overhead_tokens = 64`, `llm.ollama_keep_alive_secs = 600`.
3. **The reply** is read from `message.content`, `message.thinking` (kept as `reasoning`, which gate
   S refuses), `done_reason`, `prompt_eval_count`, `prompt_eval_cached_count`, `eval_count`.
4. **The provider's id is the model as Ollama names it** (`qwen3:1.7b`): the cache key carries it.
Evidence: `crates/oc-ai/tests/ollama.rs` (rows 11.2, 11.3 and one more).
Affects: PHASE 11 details 1–2, D10, `oc_ai::provider::ollama`, `thresholds.toml` (`llm.ollama_*`).

## 2026-09-23 · Detection and the capability probe · Phase 11
Context: PHASE 11 details 2 and 5 — Ollama auto-detected on `localhost:11434`; a health and
capability probe once per session, cached, degrading on failure; "version drift … is handled by the
capability probe rather than by version sniffing". The desktop's job spec carries an endpoint and a
model and no provider kind (§2.2), so the engine has to learn the kind from the endpoint itself.
Decisions:
1. **`Transport` gains `get`** (default: 404, for the chat-only test doubles); `HttpTransport`
   implements it with its existing GET.
2. **`oc_net::detect::detect_ollama`** asks `GET /api/tags` and returns the model names; a reply that
   is not a model list is "not detected", never an error. `ollama_transport()` is the default
   `http://localhost:11434`, which is loopback and needs no consent.
3. **`oc_net::detect::probe`** asks, in order and stopping at the first answer: `GET /props` — an
   object with `default_generation_settings` is `llama-server` (it serves `/api/tags` too, so it is
   asked first); `GET /api/tags` — Ollama; `GET /v1/models` — any other OpenAI-compatible server.
   Nothing answering is the probe's error, which the caller turns into `W_LLM_UNAVAILABLE`.
4. **PROVISIONAL — needs maintainer ratification: an OpenAI-compatible server that is neither
   `llama-server` nor Ollama is probed as constraining nothing** (`ProviderCaps::neither`, schema in
   the prompt, `W_LLM_UNCONSTRAINED`). LM Studio and vLLM document `response_format: json_schema`,
   but a `GET` cannot show that a server honours it rather than ignoring it — only a generation
   could — and claiming a constraint that silently is not applied would suppress the warning that
   tells the user why more answers fail gate S. Sending `response_format` blind also risks a 400
   from a server that rejects the field, which would disable AI for that server entirely.
5. **A base URL is reduced to its root** (`api_root`: no trailing `/`, no trailing `/v1`), so
   `https://host/v1` — how most servers document their base URL — and `https://host` both work.
Evidence: `crates/oc-net/tests/detect.rs` (row 11.1 and three more).
Affects: PHASE 11 details 2 and 5, D10, `oc_ai::transport`, `oc_net::detect`.

## 2026-09-23 · `convert --ai` opens a provider: flags, consent, probe, model · Phase 11
Context: PHASE 11 details 3–5; §2.1 lists `--llm-endpoint` and `--llm-api-key-file` and nothing that
names a provider, a model, or a consent. The job spec (§2.2) has `endpoint`, `api_key_file`,
`model_path`, `model_id`, `non_loopback_consent: bool` — and no provider kind.
Decisions:
1. **Three flags, all AI-only** (refused without `--ai`, like the others):
   `--llm-provider builtin|ollama|openai-compatible` (`builtin` is `ProviderKind::LocalSidecar`, the
   name UI_UX §2.4 gives it), `--llm-model <NAME>` (the job spec's `model_id`), and
   **`--llm-allow-host <HOST>` — the consent, and it names the host** (D10's "toggle that names the
   host"). It must equal the endpoint's host, case-insensitively; consent to another host is none.
   PROVISIONAL — needs maintainer ratification: the flag's name and that it takes the host rather
   than a bare boolean. The job spec's boolean reads as consent to its own endpoint's host
   (`AiArgs::consenting_to_the_endpoint`), the desktop dialog having named the host to the user.
2. **`E_CONSENT_REQUIRED`**, exit 2, a `fatal` whose message names the host, says nothing was sent,
   and names the flag. The check runs before the key file is read and before any connection: the
   test counts connections and finds none. Other refusals stay `E_USAGE`.
3. **The probe picks the adapter** unless `--llm-provider` does: `llama-server` → `LocalSidecar`
   (so the desktop app's own server, reached through the job spec's endpoint, gets GBNF and
   `chat_template_kwargs` exactly as in Phase 10), Ollama → `Ollama`, anything else →
   `OpenAiCompatible` with `ProviderCaps::neither`. A failed probe is `W_LLM_UNAVAILABLE` ("the
   endpoint did not answer the capability probe"), exit 0. `llm.provider_probe_timeout_millis =
   5000`, provisional. `--llm-provider ollama` without an endpoint is `http://localhost:11434`.
4. **A model is never guessed**: `--llm-model`, else the only model the server lists; several, or
   none, is `W_LLM_UNAVAILABLE` saying to name one. Ollama's `name:latest` answers to `name`. A
   `llama-server` endpoint keeps Phase 10's id (`--llm-model`, else `--model-path`'s stem, else
   `endpoint@<host>`); for the engine-owned sidecar `--llm-model` picks the registry entry.
5. **`ai_endpoint::open_with(args, registry, t, &dyn Connector)`** reaches endpoints through a
   connector — `Network` (`HttpTransport`) in the engine, an in-process double in tests — so a
   host off this machine is testable without a network. The report records the consent (P11.7).
Evidence: `crates/openconvert/tests/providers.rs` (rows 11.5, 11.8 and four more),
`ai_endpoint::the_job_specs_consent_names_the_endpoints_own_host`.
Affects: IMPLEMENTATION_PLAN §2.1/§2.2, D10, `openconvert::{ai_endpoint, cli, cmd_convert}`, Phase 12.

## 2026-09-23 · What the report says about a provider · Phase 11
Context: PHASE 11 detail 4 — "the granted consent, the host, and the timestamp are recorded in the
conversion report"; detail 5 and A11.4 — a failing provider is a deterministic book, exit 0, and a
recorded warning.
Decisions:
1. **A top-level `consent: {host, granted_at, scope}`**, present only when an endpoint off this
   machine was opened under consent (`granted_at` RFC 3339 UTC to the second, `scope: "run"`). Not
   inside `ai`: it is a fact about the run's privacy, not about the model's work, and a reader
   looking for "did text leave?" should not have to know where AI details live.
2. **`ai.provider`** names the adapter (`local_sidecar`, `ollama`, `openai_compatible`).
3. **A consent whose endpoint then failed its probe is not recorded**: nothing was opened, and the
   probe's `GET`s carry no document text. A consent whose endpoint was opened is recorded even if
   every question then failed — the question itself carried the text.
4. **Failure is the Phase 10 path, unchanged**: a probe that nothing answers is `W_LLM_UNAVAILABLE`
   ("the endpoint did not answer the capability probe"); a 500 on a question stops the session and
   is `W_LLM_UNAVAILABLE` ("the endpoint refused the request"); the book is the `--no-ai` book byte
   for byte, exit 0 — for the `llama-server`, Ollama and generic adapters alike.
Evidence: `crates/openconvert/tests/providers.rs` (rows 11.7, 11.9).
Affects: PIPELINE §13 (report), D10, `openconvert::report` (`ReportInput.{provider, consent}`).

## 2026-09-23 · `openconvert provider`: what the settings page reads · Phase 11
Context: PHASE 11's files name `crates/openconvert/src/cmd_provider.rs` and a Provider settings
page "wired in Phase 12" (UI_UX §2.4: Built-in / Ollama (detected) / Custom endpoint with the
consent dialog naming the host). §2.1 has no `provider` subcommand. The desktop app runs the engine
as a subprocess, so what the settings page needs has to be a command.
Decisions:
1. **`provider detect [--json]`** — `{"ollama": null}` or `{"ollama": {"url", "models"}}` from
   `GET http://localhost:11434/api/tags`; always exit 0. Loopback only: no consent.
2. **`provider check <URL> [--json]`** — `{url, host, loopback, requires_consent, usable, reason}`;
   sends nothing. `usable` is false only for plain http off the machine (`reason` says https). A URL
   the engine will not interpret is exit 2. This is how a UI decides to show the consent dialog.
3. **`provider probe <URL> [--llm-provider] [--llm-model] [--llm-allow-host] [--llm-api-key-file]
   [--json]`** — `convert --ai`'s own opening (`ai_endpoint::open`), so the two cannot disagree:
   `{available: true, url, host, provider, constraint, thinking, model, models, consent}` exit 0;
   `{available: false, url, reason}` exit 1; no consent → exit 2, `fatal{E_CONSENT_REQUIRED}`.
   It sends `GET`s only — no question, no document text.
4. **No network-log event is added** to the NDJSON schema (§2.3 is closed): the audit log of every
   outbound connection is PHASE 14 detail 12 (`oc-net/src/audit.rs`). What Phase 11 gives it: every
   connection `convert --ai` and `provider probe` make goes through `ai_endpoint::Connector`, and
   the report's `consent` says when text left the machine.
Evidence: `crates/openconvert/tests/providers.rs` (`provider_detect_reports_ollama_or_nothing`,
`provider_check_says_whether_consent_is_needed`, `provider_probe_answers_what_convert_would_open`).
Affects: IMPLEMENTATION_PLAN §2.1, UI_UX §2.4, Phase 12 (`routes/settings/providers.svelte`), Phase 14.

## 2026-09-23 · Phase 12 meets Phases 10, 11 and 13: one conversion path, one cache rule · Phase 12
Context: `phase/12-desktop-ui` rewrote the conversion driver for real progress, cancel and the
partial re-run (`convert_observed`, `Upstream`, `cache.rs`); main, meanwhile, split the same driver
for the AI step (`prepare` / `convert_prepared` / `finish`, Phase 10) and put OCR inside `ingest`
(Phase 13), and `convert` for the providers (Phase 11). Merging `origin/main` (c812e9e) into the
branch had to keep every behaviour of both.
Decisions:
1. **One driver.** `convert` → `convert_with_ai` → `convert_observed(…, ai, t, observe)`: `ingest`
   (pages read one at a time, then OCR) → `text` → `furniture` → `layout` → `structure` (+ the AI
   step, inside `structure` as a user sees it) → `document` → … . `prepare` and `convert_prepared`
   stay public for the tests that change a book's evidence before `structure`. The job spec and
   `convert` resolve to one `ConvertJob`, which now carries `--ai` and the four OCR flags;
   `hello` carries `ocr:tesseract-…` from discovery on both paths (a job spec reads scanned pages
   the way `convert`'s default `--ocr auto` does). Image decoding uses main's `slots` (never the
   position among a page's images, which OCR changes).
2. **The partial re-run saves only what it can restore whole.** A run that asked a model, or in which
   OCR read, refused or warned about anything, is neither saved nor resumed: the model's answers and
   OCR's report are not in the save, and a rebuild whose report silently dropped them would describe
   another book. The escalation records *are* saved (`EscalationRecord` now owns its `task` and
   `predicate` text so it can be read back; the JSON is unchanged), so a born-digital book keeps its
   shortcut. The cache key gained the OCR settings (mode, re-OCR, languages, engine); `ingest` joined
   the stages a saved ledger may name; `ReasonTotals::replay` carries what OCR added (I-6).
3. **Duplicate thresholds**: both branches added `llm.load_timeout_secs` and
   `llm.health_probe_timeout_millis` (the desktop app's server wait; `convert --ai`'s). One wait, one
   key: main's values (300 s, 2000 ms) are kept and the desktop app's `llm.rs` reads them.
4. **A deadlock the merge exposed.** `main.rs` held stderr's lock on the main thread for the whole
   run; the heartbeat thread's first write blocked on it while holding the event sink, and the run's
   next event blocked on the sink. Every run longer than `ipc.heartbeat_secs` hung — never seen on
   the branch because its fixtures finish inside one period, seen at once with OCR. The top-level
   sink now writes to an unlocked `Stderr`
   (`a_run_longer_than_the_heartbeat_period_beats_and_keeps_reporting`).
Evidence: the whole workspace suite on the merge (740 tests), `report_f07` snapshot recounted to 224
threshold entries.
Affects: `crates/openconvert/src/{convert,cache,cmd_convert,cmd_job,main,cli}.rs`,
`crates/oc-structure/src/escalate.rs`, `crates/oc-core/src/ledger_check.rs`, `thresholds.toml`.

## 2026-09-23 · The job spec's `ai` object is `convert --ai` · Phase 12 (P12.17)
Context: the engine's job-spec reader refused `ai.enabled` by name until Phase 10 existed; PHASE 11's
hand-off names the mapping; the desktop app spawns the engine with one argument (12.1), so AI has to
travel in the spec.
Decision: `ai.enabled = true` resolves to the same `AiArgs` `convert --ai` builds — `endpoint` →
`--llm-endpoint`, `api_key_file` → `--llm-api-key-file`, `model_path` → `--model-path`, `model_id` →
`--llm-model`, `non_loopback_consent: true` → `--llm-allow-host <the endpoint's own host>`
(`AiArgs::consenting_to_the_endpoint`). `enabled: false` is AI off whatever else the object holds.
There is no provider-kind field in job-spec v1: the engine probes. There is no `all_tasks` either,
so the app runs only the tasks `ai.task.<task>.languages` enables — none, in this build.
Evidence: `a_job_spec_with_ai_on_asks_the_endpoint_it_names`,
`a_job_spec_with_ai_on_and_no_model_converts_without_one`, and three `cmd_job` unit tests.
Affects: `crates/openconvert/src/cmd_job.rs`, the `<JOB.json>` usage text.

## 2026-09-23 · The AI switch, and the app's own server per job · Phase 12 (P12.18)
Context: PHASE 12 part B2 item 1 (PROGRESS): persist the choice; for each job with AI on, lease the
app's `llama-server` for the installed default model and write `ai.enabled`/`endpoint`/
`api_key_file`/`model_id` into the spec; release when the job ends; the fail-open banner (UI_UX §4);
the decision count (UI_UX §2.3). Settings › AI assistance was drawn disabled until the engine could
read `ai` (P12.17).
Decisions:
1. **A job takes the AI settings of the moment it is queued** (`ai::JobAi::plan`): off, built-in, an
   endpoint (Ollama or custom, written as configured), consent required, or unusable. A setting
   changed later does not reach a job already waiting.
2. **Built-in waits in the queue, off its lock.** When its turn comes the job is `preparing`: a thread
   leases the server (`ai::ModelServer`, `llm::AppModelServer` over `LlmHost`) while the queue keeps
   answering Cancel; then its engine starts with the lease in its spec. The lease is released when
   the job ends (exit, kill, a failed start), so `llm.idle_kill_secs` counts from the last job. The
   supervisor clock only *tries* the server's lock, so a loading model never stops the queue's clock;
   at exit a server still loading is ended through `supervise::kill_all`.
3. **Fail-open, said once.** No model installed, no `llama-server` in this build, a server that does not
   come up, a custom endpoint that is not a URL or is plain http off this computer: the book converts
   without AI and the row's result carries the banner "AI assistance unavailable this run — converted
   deterministically." with the app's reason; the engine's own `W_LLM_UNAVAILABLE` raises the same
   banner and its warning line says why.
4. **The switch says what it does in this build.** Every `ai.task.<task>.languages` ships empty and the
   job spec has no `all_tasks`, so AI on changes no book yet; `UiConfig.aiTasksEnabled` (from those
   four keys) makes Settings say so instead of promising more. With AI on the result reads "AI-assisted
   decisions: N" in place of "Deterministic processing" (UI_UX §2.3).
5. **The key file and the consent are the Rust side's.** `settings_set` keeps both as they were
   (`Settings::merged_from_webview`) and withdraws a consent when the endpoint's host changes.
6. **PROVISIONAL — needs maintainer ratification: built-in serves the registry's default model only.**
   UI_UX §4's "falls back to the next-safest installed tier" when a model fails its gates at load is
   not built: "next-safest" has no ordering in the registry, and no model can be fetched here to fail
   one. A server that does not come up is the fail-open row above.
7. A rebuild ("Fix and rebuild") with AI on is a full run: the engine never resumes a run that asks a
   model (merge decision 2).
Evidence: `built_in_ai_leases_the_app_server_for_the_job_and_releases_it_at_the_end`,
`built_in_ai_without_a_model_converts_without_ai_and_says_why`,
`a_job_cancelled_while_the_model_loads_never_starts`, `a_host_nobody_consented_to_never_starts`, the
four `ai::tests`, `the_webview_cannot_write_a_key_file_or_a_consent`, and — against the real engine and
`oc-stub-llama-server`, which now answers `/props` as `llama-server` does —
`a_conversion_with_built_in_ai_is_served_by_the_apps_own_server`; UI: the settings, result and
jobstate tests named in the commit.
Affects: `apps/desktop/src-tauri/src/{ai,jobqueue,llm,models,settings,config,main}.rs`,
`apps/desktop/ui/src/{routes/settings/Settings.svelte,components/{ResultPanel,QueueRow}.svelte,lib/*}`,
`locales/*.json`, `crates/oc-testkit/src/bin/oc-stub-llama-server.rs`.

## 2026-09-23 · Settings › Provider and the consent dialog · Phase 12 (P12.19)
Context: PHASE 11's hand-off ("What Phase 12 must wire from Phase 11") and UI_UX §2.4: Built-in /
Ollama (detected) / Custom endpoint; the consent dialog naming the host, saying plainly that document
text leaves the computer (D10); `E_CONSENT_REQUIRED` re-opens it; the webview keeps
`connect-src 'none'`.
Decisions:
1. **The engine answers every provider question** (`providers::ProviderCli` → `openconvert provider
   detect|check|probe --json --progress json`), off the main thread. These are not conversions, so
   they take more than one argument; the command is built in Rust, and the one value the webview
   supplies — a URL — must be an `http(s)://` URL with no control character, so it can never be read
   as a flag. A refusal is read by its NDJSON code, never by its prose.
2. **Built-in and Ollama are chosen at once; a custom endpoint only through "Use this endpoint…"**:
   `provider check` (sends nothing) → plain http off this computer is refused in the user's language;
   a host off this computer without consent opens the dialog; Allow asks the Rust side to record the
   consent (`grant_consent`, `Settings::granting_consent`: host + RFC 3339 time) and selects the
   endpoint; Cancel records nothing. The key file is picked in a native dialog by the Rust side
   (`pick_key_file`) and is never read by the app.
3. **"Test connection"** is `provider probe` with the saved settings (the consent passed only to the
   host it names); built-in has nothing to probe until a job starts it.
4. **`E_CONSENT_REQUIRED`, or the queue's `consent_required` refusal, opens the dialog again** — once by
   itself, and from the row's "Review consent…"; the row says "Sending text to another computer needs
   your consent · Nothing was sent.", never a generic error. Allow converts that book again.
Evidence: `a_provider_question_names_its_url_as_one_argument_and_refuses_anything_else`,
`test_connection_probes_what_a_conversion_would_open`, `a_consent_refusal_names_its_host`,
`the_providers_screen_reads_the_engines_answers` (engine-integration, the real engine); UI: the two
provider-screen tests and the two consent tests in `queue.svelte.test.ts`.
Affects: `apps/desktop/src-tauri/src/{providers,main}.rs`, `apps/desktop/ui/src/{routes/settings/
Settings.svelte,components/{ConsentDialog,QueueRow}.svelte,App.svelte,lib/{backend,jobstate}.ts}`,
`locales/*.json`, `tests/dom/ui/tauri-mock.ts`.

## 2026-09-23 · Settings › Network log: a hook for PHASE 14's audit log, not a stand-in · Phase 12 (P12.21)
Context: the Network log section is the design's own decision (`docs/design/handoff/docs/
decisions.md` 6, PHASE 12 "Visual design" 6) and SECURITY §8's auditability promise; the log it lists
is `oc-net`'s audit log, `{ts, host, purpose, bytes, outcome}` per connection — PHASE 14 detail 12,
not built. Phase 11 added no event for it (§2.3 is closed). Part B1's wording said the app connects
only for downloads; with Phase 11 wired it also connects, when AI is on, to the chosen provider.
Decision: `netlog::read()` answers `NotRecorded`, always, behind a command (`network_log`) and a page
already wired for `Entries` (a table: when, host, why, data). The page says this build records
nothing yet and names every connection it can make: a model or pack download (only to the
registry's pinned host), and with AI on the provider chosen (the model server or Ollama on this
computer, or a custom endpoint the user allowed). No row is invented and nothing is counted. PHASE 14
replaces `read()`'s body with a reader of `<data_dir>/network-audit.log`.
Evidence: `the_network_log_says_it_is_not_recorded_until_there_is_an_audit_log`; UI: the network
log says this build records nothing yet, and lists what the audit log holds.
Affects: `apps/desktop/src-tauri/src/{netlog,main}.rs`, `Settings.svelte`, `locales/*.json`,
PHASE 14 detail 12 (fills the hook).

## 2026-09-23 · The engine sidecar is `openconvert-engine`, not `openconvert` · Phase 15 (P15.1)
Context: carry-over from Phase 12. `tauri-build` copies every `externalBin` into `target/<profile>/`
without its triple, removing whatever file is there first. The sidecar was `bin/openconvert`, so
building the desktop crate replaced `target/debug/openconvert` — the engine every workspace test runs
— with whichever build was last staged, and cargo, whose fingerprint for the engine was still fresh,
never put the new one back (seen here: `target/debug/openconvert` had one link, the staged copy,
where cargo's own output has two). Re-running `stage-sidecars` was the only cure, and nothing said so.
Decision: the engine is staged and bundled as `openconvert-engine` (`xtask::stage_sidecars::
SIDECAR_NAME`, `openconvert_desktop::engine::SIDECAR_NAME`); the binary cargo builds, and the CLI a
user runs, stays `openconvert`. `stage-sidecars` removes a legacy `bin/openconvert-<triple>`. No
sidecar may share a name with any workspace binary, whatever the build order, and a test enforces it
over every Tauri config file. **PROVISIONAL — needs maintainer ratification**: the plan says "the
`openconvert` engine … as `externalBin`"; the file name inside the bundle is not settled anywhere, and
this is the narrowest name that keeps the engine recognisable.
Evidence: `no_sidecar_shares_a_name_with_a_workspace_binary` (RED on `bin/openconvert`),
`the_sidecar_the_app_runs_is_the_one_tauri_bundles`.
Affects: `apps/desktop/src-tauri/{tauri.conf.json,src/engine.rs}`, `xtask/src/stage_sidecars.rs`.
