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
