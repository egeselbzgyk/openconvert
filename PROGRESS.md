# PROGRESS — OpenConvert

<!-- Machine-readable state. Claude Code reads this first and rewrites it after every completed work item. -->

STATUS: IN_PROGRESS
CURRENT_PHASE: 7.5
CURRENT_ITEM: 7.5.0 — the reading corpus: 40-50 novels per language across en/de/tr
              (rows 7.5.0a-7.5.0c), then 7.5.1 the diff-stage diagnostic
LAST_UPDATED: 2026-09-20

---

## How to use this file

- `STATUS` is one of `IN_PROGRESS` · `BLOCKED` · `COMPLETE`.
- Set `STATUS: BLOCKED` **only** when a decision is needed that `docs/DECISIONS.md` does not settle.
  Write the question under `## Phase 7 — what it has built so far

- **`oc_eval.corpus.download`** fetches mirror-first, verifies the manifest's sha256 before the
  bytes are placed, and deletes a file that fails rather than quarantining it. The opener is a
  parameter, so no test needs a connection; the default refuses any URL that is not https.
- **`oc_eval.corpus.manifest`** is the vocabulary — licence allowlist and blocklist markers, the
  nine producer strata plus `page-level-layout`, and the document-vs-page unit.
  **`oc_eval.corpus.lint`** is fourteen rules over it, one stable slug each, and the slugs are
  the contract the tests assert on.
- **`oc_eval.thresholds`** reads `thresholds.toml`, so the corpus gates are the same numbers in
  Python and in Rust rather than two that agree today.
- Two rules needed a judgement the plan does not make. `tagged-share-off-target` is skipped when
  the synthetic bucket is too small to land within five points of 0.126 at all — a five-file
  bucket can be 0.0 or 0.2 tagged and nothing between, and reporting that is reporting
  arithmetic. `is_page_level` deliberately does not trust the `unit` field alone: a DocLayNet
  entry is page-level whether or not it says so, and `page-unit-undeclared` separately requires
  it to say so, which is what closes §7.6's per-page shortcut.
- **`mypy` is clean on everything Phase 7 has written** and reports 13 errors in two Phase 2
  files (`generate/scan_sim.py`, `train/hyphen_clf.py`). Item 7.10 owns them, because that is
  where `mypy` becomes a CI gate.

### Item 7.3 — how the corpus was assembled

- **Five adapters, each a pure parser plus a thin fetch loop.** OAPEN's DSpace
  `/rest/filtered-items` filters on `dc.rights.uri` and `dc.language`, so the CC-BY subset and
  the German slice are selected by the catalogue rather than by downloading and hoping.
  Internet Archive filters on `licenseurl` and then picks the scan out of an item's files,
  avoiding the `_text.pdf` and `_djvu.pdf` sidecars. arXiv reads OAI-PMH `arXivRaw`, because
  the Atom search API reports no licence at all and a paper with none is under arXiv's own
  distribution licence, not ours. US-Gov is NASA's NTRS, admitting only
  `copyright.determinationType == "GOV_PUBLIC_USE_PERMITTED"`.
- **The Turkish slice was rewritten mid-item.** Reading the licence off a DergiPark article
  page admitted one article in ten — most DergiPark journals are CC BY-NC-ND. Selecting
  **journals** from DOAJ by their curated `license.type` and then taking a few articles from
  each admits at the journal's rate instead, and DOAJ's `fulltext` link for those journals is
  the direct PDF, so nothing scrapes a page any more.
- **Nothing enters the manifest on a promise.** Every entry's `sha256`, `pages`, `tagged` and
  `producer_raw` come from opening the file that actually arrived. 45 candidates were rejected
  with a named reason across the four runs — duplicates, landing pages that were not PDFs,
  files over the 40 MB per-file budget, documents under four pages.
- **A top-up harvest needed a fix to work at all.** A source adapter re-walks its catalogue
  from the start, so `usgov=2` against thirteen NASA reports already held yielded thirteen
  duplicates and nothing else. `admit(want=…)` caps what one call admits and `ask_for` clears
  the held count, so the generator is drawn further down the catalogue and stops as soon as it
  has enough.

### Item 7.4 — the mutation catalogue

Ten recipes in `oc-testkit::mutate`, listed as data in `xtask::mutations::catalogue()` so that
row 7.6's "every recipe" has one place to be asked. Five are new: `strip_structtree`,
`double_draw`, `ocr_sandwich`, `jitter_spacing`, `damage_xref`.

Each recipe declares two things the tests hold it to.

- **`Reproducibility`.** Running `xtask mutations` twice showed the three encrypted mutants
  rewritten with different bytes every time — 902 bytes on one run, 903 on the next — because
  AES-128 draws a fresh initialisation vector per string and stream. Row 7.6's byte-for-byte
  assertion holds for the seven `Deterministic` recipes; the three `Randomised` ones are
  asserted on the property that makes byte-equality impossible, that two applications differ.
  `xtask mutations` now leaves an existing randomised mutant alone, because regenerating it
  replaced a regression artefact with noise.
- **`Effect`** — what the recipe does to the characters on the page, checked through PDFium
  against the parent. `Preserves` (cropbox offset, struct-tree strip, jitter, damaged xref),
  `Duplicates` (double draw, OCR sandwich — exactly twice the characters, the original first),
  `BreaksTextMapping` (ToUnicode strip), `Unopenable` (encrypted with a user password).
  Flipping one declared effect turns the test red, which is how it is known not to be vacuous.

**Type 3 re-encoding is the one item of PHASE 7 §4's list that is not done.** Re-encoding an
embedded font as Type 3 while keeping its outlines needs a glyph-outline extractor `lopdf` does
not have, and a Type 3 font whose CharProcs draw rectangles would change what the page looks
like rather than only how it is encoded. It belongs with the handmade fixtures — a small PDF
authored as Type 3 with a `/ToUnicode` map — and is a gap, not a mutation.
`docs/DECISIONS_LOG.md`, 2026-09-20.

### Item 7.5 — ground truth

Three sources, one type. `oc_eval.ground_truth.schema.GroundTruth` is TEST_CORPUS §5.1's shape
— headings, paragraphs, footnote pairs, figures — whatever produced it, so the scorer has one
thing to compare against and a fourth source would not touch it.

**The assertions are the engine's own vocabulary.** `to_assertions` emits `heading_tree`,
`text_present`, `block_count`, `image_count`, `note_bijection`, `lang_tag` and `text_order` —
the same kinds `oc_testkit::assertions` reads and the committed `.assert.json` fixtures are
written in — so one runner checks a generated expectation and a hand-written one.
`test_every_generated_assertion_kind_is_one_the_engine_knows` reads the Rust enum rather than
keeping a second copy of the list, the same trick `xtask ci-lint` uses for the warning registry.

**A ground truth never invents what its source does not say.** A tagged PDF's `/H1` points at
marked content, not at characters, so a struct-tree heading has a level and no text — and
`to_assertions` emits no `heading_tree` or `heading_level` for it, because asserting on an
empty string would score every document as wrong. The same rule drops a noteref whose endnote
is in a file that was not read, and withholds `note_bijection` when the notes do not pair.

- `from_xhtml` reads Standard Ebooks markup. Heading levels come from `<section>` nesting, not
  from the tag number: SE writes a chapter title as `<h2>` because the book's `<h1>` is its
  title page, and comparing `h2` against `h1` would score a correct conversion as wrong.
- `from_structtree` reads a tagged PDF's own tree. On the tagged fixture it finds one `/H1`
  and four `/P`, which is the document.
- `from_latex` reads arXiv sectioning, and `looks_parseable` is §5.2's "curated
  parses-cleanly subset" as a predicate — a paper that pulls its sections in through `\input`
  is refused **before** it is scored against rather than after it has quietly scored zero.

`corpus/gt/se_sample/` is a small SE-shaped book written for this repository: a chapter with
two heading levels, four paragraphs, two noteref/endnote pairs and a captioned figure.

### Item 7.6 — the metric suite

Eight modules under `oc_eval.metrics`, TEST_STRATEGY §8.1's table one function at a time:
`cer` (text NED after D13.4's normalisation), `reading_order` (edit similarity plus Kendall
tau), `toc_f1` (heading P/R/F1 and outline edit distance), `footnotes`, `images`, `teds`
(TEDS and TEDS-S), `prf` (the shape the four set-metrics share) and `report`.

Two decisions carry the phase's weight.

- **A pass rate is an interval, not a number.** 94 of 100 and 940 of 1000 are the same rate
  and not the same evidence. `assertions.PassRate.interval()` is Wilson's, pinned against the
  published value for 95/100 — (0.8882, 0.9785) — because the normal approximation is wrong
  exactly where this gate lives, at p near 1. The gate is last-green minus the half-width, and
  a suite below `eval.assertion_min_instances` **fails and says why** rather than passing on
  an interval wide enough to admit any regression.
- **There is no aggregate row and there never will be.** `report.build` emits `per_file`,
  `per_stratum` and `ours_vs_real`, and a test asserts no `aggregate` key appears — an average
  across strata is precisely the number that lets a synthetic win mask a real-book regression,
  which is what D18 exists to prevent. A report with no real strata states no gap rather than
  inventing one.

The metrics normalise before they measure. `cer.ned("ﬁre", "fire")` is 0.0 and
`cer.ned("pipe-
line", "pipeline")` is 0.0, because the pipeline is *supposed* to fold those
(D13.4's `N`) and a metric that charges for them scores a correct conversion as wrong — R9
§A.10's gap in Nougat's metric. What is not folded is anything the pipeline may not change: a
hyphen inside a line is content and still costs.

Two new thresholds: `eval.assertion_confidence` (0.95, binary — the plan's level) and
`eval.assertion_min_instances` (30, provisional — where the Wilson half-width at p ≈ 0.95
falls under eight points).

### Item 7.7 — the trend

`oc_eval.trend` keeps the `ours(*)`-versus-real gap in `eval/out/trend.json`, in the
repository, because TEST_STRATEGY §8 wants a regression to be a diff against the
immediately-prior committed baseline. One entry per commit: re-running the nightly on an
unchanged tree corrects the record rather than doubling it.

The point is the **direction**, not the number. A gap of 0.06 is fine or alarming depending on
whether last week's was 0.05 or 0.09, so `widening()` reports first-to-last and returns None
when the history holds fewer than two runs that stated a gap at all — a run with no real strata
states no gap, and is not evidence about widening either. `plot()` draws it to a PNG under a
headless backend.

### Item 7.8 — the holdout refusal

TEST_CORPUS §7.1(c) as a mechanism rather than a sentence. Every fit goes through
`calibrate.fit`, which loads the manifest and raises `HoldoutLeak` on any id marked `holdout`.
Two details make it hold:

- **an unknown id is refused too** (`UnknownFile`). An id the manifest cannot account for is
  not evidence that it is not holdout, and "I could not check" must not read as "it is fine";
- **one offending file stops the whole fit.** Dropping it and carrying on would produce a
  number that looks fitted on what was asked for and was not.

`risk_coverage` is where an escalation threshold actually comes from (D17): sort by
confidence, and report the widest coverage whose risk is still under the target — returning
None when nothing meets it, rather than the best available dressed up as a hit. `reliability`
is the ECE and the diagram rows behind it.

### Item 7.9 — the performance budget

**Measured: 0.0217 s/page over 300 pages**, against D13.11's 0.5 — a 23x margin, in an
unoptimised `test` profile on the maintainer's machine. The gate was confirmed to fail when the
budget was lowered below the measurement, so it is not vacuous. Machine L's number will differ;
this is a floor on the headroom, not the reference measurement.

`oc_testkit::handmade::reference_book(pages)` builds the input rather than committing it: three
hundred pages of prose is a megabyte of fixture nobody would regenerate or review. It is
deterministic, and it carries what makes a book *expensive* rather than merely long — a running
head and a folio on every page for furniture detection to find, a chapter opening every twenty
pages, body lines at a real leading.

The split: the **arithmetic** runs on every PR — that row 7.12's five stage budgets sum to
`perf.seconds_per_page_max`, that each is a positive share of it, that the reference book is the
300 pages D13.11 states the budget for — because that is where the mistake that actually happens
gets caught, a stage quietly given room the whole does not have. The **timed** assertions are
behind the `bench` cargo feature, which the nightly job turns on: a wall-clock assertion on a
shared CI runner measures the runner, and a gate that fails for that reason is one people learn
to re-run until it passes.

`oc_eval.bench.peak_rss` measures the **whole process tree** — `/proc`'s `VmHWM` on Linux, a Job
Object on Windows, `psutil` polling otherwise — because D13.11's 500 MB is for the converter and
whatever it spawns, and a figure that counts only the parent stops being true the moment Phase
9's sidecar exists. It says which mechanism it used and whether the answer is exact, and a
sampled floor is printed as a floor.

Six new thresholds: the five per-stage budgets and `perf.bench_reference_pages`.

### Phase 7.5 — where to resume (written for a fresh session)

The plan section is `docs/IMPLEMENTATION_PLAN.md` PHASE 7.5. Read it; this is only the state.

**Two things are true and both drive the phase.**

1. **The pipeline converts nothing.** A random sample of 14 Phase 7 holdout documents gave
   11 I-1 refusals — **every one in `structure`** — losses 48 … 203 205 characters, plus 2
   timeouts at 180 s. Minimal reproducer already isolated:
   `corpus/downloads/oapen-20-500-12657-115632.pdf` **page 42 alone** loses 644 characters.
2. **Phase 7's corpus is the wrong population.** Open-access monographs, papers, government
   reports and library scans — chosen for licence clearability. This product converts PDFs
   into books people read. `--preset novel` exists in §2.1 and no novel has ever gone through
   the pipeline.

**The work item in progress is 7.5.0, the reading corpus**: 40–50 novels per language across
English, German and Turkish, same licence discipline, **not marked holdout** so the phase may
fit on them.

Three agents produced candidate lists on 2026-09-20; the seeds are committed at
`corpus/reading/candidates_{en,de,tr}.json` and the admitted subsets beside them. Every list
was re-verified here, because none of the agents' own licence fields were read from the source:

```
English  16 of 17 admitted    (rejected: Fitzgerald's 1961 Odyssey, in a lending collection)
German   23 of 24 admitted    (rejected: an untraced German translation)
Turkish   4 of 10 admitted    (rejected: 5 in copyright, 1 a Chagatai manuscript)
```

**The Turkish number is structural, not a sourcing failure.** Copyright is life + 70 and the
alphabet reform was 1928, so a Latin-script Turkish novel is public domain only if its author
wrote after 1928 and died before 1956 — a 28-year window. *Çalıkuşu* misses by one year and
enters on 1 January 2027. Source the slice from inside the window — **Sabahattin Ali (d. 1948)
and Sait Faik Abasıyanık (d. 1954)** are the two widely-read authors in it — and if it cannot
reach 40, report it short with the reason rather than padding it with Ottoman-script scans,
which answer a different question.

Still to do for 7.5.0: the German list is Google-Books Fraktur and several entries are plays
rather than prose, so it wants filtering and topping up; English needs ~24 more; the
`PD-old-work` admission path itself (rows 7.5.0x–z) is written in the plan and not yet in
`stratify.py`. The verification script used here is in the session scratchpad and should be
rewritten as `eval/src/oc_eval/corpus/sources/public_domain.py`.

**The rule that governs every fix in this phase, and the reason it is a phase:**
a fix may not be derived from a single document. The unit of work is a defect *class*
`(stage, direction, signature)`, admitted only at **≥ 3 documents across ≥ 2 strata** and, for
a reading-corpus class, **≥ 2 languages**. Every class carries a written *How else could this
arise?* answered **before** any code changes. A class closes by making the fault
unrepresentable or checked — never by special-casing the shape it was found in — and leaves an
invariant test, not only a corpus file.

**Two things no item in this phase may do**: fix a document instead of a class, and make a
refusal disappear by widening a `conservation.budget.*` or adding a `Reason` meaning "text we
could not account for". The second would turn a refusal into a silent loss, which is worse
than shipping nothing.

**The first class is already visible and is the template.** `oc_structure::stage` keeps a
`taken: BTreeSet<BlockId>` fed from four independent sources — note blocks, `tables.consumed`,
`lists.consumed`, bound captions. A block in `taken` is dropped from the flow on the assumption
that whatever claimed it re-emits its text, and **nothing checks that assumption**. The
architectural answer is a `Claim { block, by, text }` carrying its obligation, with the
invariant stated in `oc-core` over the IR rather than inside `oc-structure` — because the same
shape exists in `document`, `epub` and `repair`, and stating it once fixes four stages.

**The LLM boundary is one question**, and `docs/LLM_BOUNDARY.md` is item 7.5.7's deliverable:
*if this is answered wrongly, does the book lose or gain a character?* Yes → deterministic,
permanently. No → it is a name, and a name may be escalated (D13.6's four tasks). A property
test asserts no permitted LLM edit changes `C(D)`.

## Blocked` and stop.
- Tick a phase box only when its full Definition of Done (`IMPLEMENTATION_PLAN.md` §0.3) passes.
- Keep `## Notes` short: what a fresh session needs in order to resume, nothing else.

---

## Phases

- [x] **Phase 0** — Repo, workspace, CI, thresholds, hello-Tauri, first Typst fixtures
      *(First Milestone met; opened the Verification-debt table VD-a…VD-g)*
- [x] **Phase 1** — PDF inspection and ingestion  *(VD-d closed)*
- [x] **Phase 2** — Text assembly and normalization  *(all 22 named tests green; EN frequency list ships, DE/TR blocked on a D15 licence decision)*
- [x] **Phase 3** — Layout  *(all 17 named tests green; VD-b closed. A3.2 and A3.3 partial — both want the Phase 7 corpus)*
- [x] **Phase 4** — Structure  *(all 21 named tests green, plus about fifty additions; A4.3 partial — heading F1 needs the Phase 7 corpus)*
- [x] **Phase 5** — EPUB generation, Tier-1 validator, EPUBCheck CI gate
      *(all 20 named tests green, plus about sixty additions. **EPUBCheck 5.3.0 reports 0 errors
      and 0 warnings on all ten fixtures.** A5.3 is a CI job that cannot run on one machine and is
      unverified until the first CI run.)*
- [x] **Phase 6** — Structural validation, repair loop, report, CI DOM checks
      *(all 16 named tests green, plus about sixty additions. **I-7 holds on all ten fixtures, zero
      repairs fire, and 198 Chromium assertions pass at three viewports.** The two rows that could
      not be verified on one machine are **cashed**: CI run 35462059355 is green on
      ubuntu/macos/windows and `dom-checks` passes. Both of them failed first, along with five other
      real defects the first CI run found — `docs/DECISIONS_LOG.md`, 2026-09-19. VD-f deferred to
      Phase 15 with its reason.)*
- [x] **Phase 7** — Corpus v1, eval harness, benchmarks, real-world holdout
      *(all 14 named tests green, plus about 180 additions. **104 frozen holdout documents
      across 17 291 pages and seven producer strata, `ours(*)` at 0.103, corpus lint clean,
      and 0.0217 s/page against D13.11's 0.5 budget.** The CI jobs this phase wrote cannot be
      verified on one machine and are unverified until the branch merges.)*
- [ ] **Phase 7.5** — Reading corpus and conservation defect closure  *(added 2026-09-20,
      after Phase 7's first corpus run converted 0 of 14 sampled documents. Appendix D's
      "I-1 … I-7 hold on 100 % of the corpus" is a v1.0 release gate and does not hold —
      and Phase 7's corpus is monographs and papers, not the novels this product is for.)*
- [ ] **Phase 8** — AI abstraction (no real model yet)
- [ ] **Phase 9** — Local model integration: sidecar lifecycle, model manager, promotion gate
- [ ] **Phase 10** — AI-assisted decisions (the four tasks)
- [ ] **Phase 11** — BYO providers
- [ ] **Phase 12** — Desktop UI  *(includes the early signing/notarization dry run)*
- [ ] **Phase 13** — OCR  *(VD-g must close)*
- [ ] **Phase 14** — Security hardening
- [ ] **Phase 15** — Packaging & release  *(then check Appendix D: Definition of Done for v1.0)*

## Current work item

**Phase 7 is complete, and its first corpus run is why Phase 7.5 now exists.**

A random sample of 14 holdout documents converted **zero** of them:

```
11  I-1 refusal, every one of them in `structure`, losses 48 … 203 205 characters
 2  timeout at 180 s (both Internet Archive scans)
 1  harness error (a /dev/null output path on Windows), not a pipeline failure
```

All eleven refusals are the same stage and the same invariant, so this is one systematic
defect — or a small number of them — rather than a diffuse quality problem. **The conservation
law is working**: every other converter would have emitted these books short and said nothing.
But Appendix D's first correctness item, "I-1 … I-7 hold on 100 % of the corpus", is a v1.0
release gate, and Phases 8–15 had no place to do that work.

**Phase 7.5 is that place**, and its governing rule is that a fix may not be derived from a
single document: the unit of work is a defect *class*, admitted only at ≥ 3 documents across
≥ 2 strata, closed only by an architectural change plus an invariant test. The plan section
states the rule, the admission threshold, the required *how else could this arise?* step, and
the two failure modes it is written against — fixing books instead of classes, and making a
refusal disappear by widening a `conservation.budget.*`.

First step for Phase 7.5: read `docs/IMPLEMENTATION_PLAN.md` PHASE 7.5, then take item 7.5.1
with the TDD loop. It is the diagnostic, and it is first because its absence is itself a
finding — locating one defect cost a hand-written binary search over page prefixes.

**The corpus exists** and **the mutation catalogue is complete.** `corpus/manifest.json` holds
116 entries: 104 frozen holdout documents across 17 291 pages, and 12 synthetic fixtures.
`oc-eval corpus lint` is clean.

```
ABBYY-scanner  n=16  holdout=16   InDesign  n=12  holdout=12   Word     n=14  holdout=14
Ghostscript    n= 1  holdout= 1   pdfTeX    n=14  holdout=14   unknown  n=47  holdout=47
ours(Typst)    n=12  holdout= 0
TOTAL        n=116  holdout-documents=104  ours-share=0.103
```

TEST_CORPUS §7.6's sourcing target is met as written — OAPEN 44 (~40), Internet Archive 20
(~20), arXiv 15 (~15), US-Gov 15 (~15), DergiPark 10 (~10) — and the two slices §7.6 says may
not be dropped are there: 34 German documents and 10 Turkish. Licences: CC-BY-4.0 66,
PD-old-work 20, PD-US-Gov 15, CC-BY-SA-4.0 3.

Work items for Phase 7, in order. Each is one TDD loop and one commit:

- [x] **7.1** `corpus/download.py` + `oc_eval.corpus.download`: mirror-then-source, sha256 before
      use, the `LOCAL_EVAL_ONLY` boundary (row 7.5)
- [x] **7.2** `oc_eval.corpus.{manifest,lint}` + `oc-eval corpus lint|stats` (rows 7.1, 7.3b)
- [x] **7.3** corpus v1: 104 holdout documents to TEST_CORPUS §7.6's shape; rows 7.2 and 7.3
      are gates over the real manifest and the whole lint is clean (A7.1, A7.1b)
- [x] **7.4** the mutation catalogue: ten recipes, each with a declared effect (row 7.6)
- [x] **7.5** ground truth from three sources, emitting the engine's own assertions (row 7.7)
- [x] **7.6** the metric suite, the Wilson-interval gate and the per-stratum report (7.8, 7.9)
- [x] **7.7** the ours-vs-real gap recorded and plotted over time (row 7.10)
- [x] **7.8** `oc-eval calibrate` refuses the holdout, and the curves it fits (row 7.4)
- [x] **7.9** the performance budget, measured at 0.0217 s/page on 300 pages (7.11, 7.12)
- [x] **7.10** CI: the `python` and `corpus-lint` jobs, and the four nightly bodies (7.13, 7.14)

Phase 7 is the corpus, the eval harness, the benchmarks and the real-world holdout — the phase every
earlier one has been deferring to. What waits on it, in the order it will be wanted:

1. **`validate.min_char_retention` cannot be a gate as written.** Retention counts ledgered furniture
   removal as loss, so the 0.98 floor and the 0.04 furniture budget are jointly unsatisfiable for a
   book with a running head. Either the floor moves to `1 − global_non_ocr_removal` (0.92), or the
   metric becomes retention of text *no reason accounts for* — which is I-7, and would make the second
   gate redundant. Appendix D's v1.0 item needs the strata to choose. `docs/DECISIONS_LOG.md`,
   2026-09-18.
2. **`validate.h1_count_min_pages = 20` means every fixture answers `None`** to the h1-count
   plausibility question. The check is written and tested and is first exercised for real on the
   corpus.
3. **`validate.dup_block_frac = 0.02` has met no real book.** Derived from the *AI Engineering* shape
   (21 526 characters emitted twice), not measured.
4. **Two of four real books outside the corpus are still refused by the conservation law**, both in
   `structure`, and the table thresholds are all `provisional`. See the Notes below.
5. **A2.3, A3.2, A3.3 and A4.3 are all partial on one cause**: no corpus, no gold data.
6. **The benchmark harness** is what turns A1.6 (0.5 s/page, 500 MB RSS on D9's reference machine L)
   from an indication into a measurement.
7. **`oc-eval bench report` prints repair fires per id per stratum** (RT A10.4). The loop counts them
   already; nothing aggregates them yet.

## Phase 6 — what it built

**Phase 6 is complete.** The structural validator (invariant I-7 end to end), the validate→repair
loop, the conversion report, and the CI DOM checks. It is the first phase whose subject is *what to
do when the output is wrong*.

Work items, in order, with the plan's test rows against each:

- [x] **6.1** I-7 and retention — `oc-validate::structural` (rows 6.1, 6.2, 6.3 + 6.1a/6.1b/6.2a/6.3a)
- [x] **6.2** the rest of the structural checks: image parity, note bijection, heading-tree
      sanity, duplicate and quality statistics (row 6.16 + additions)
- [x] **6.3** the repair loop: measure, static table, plan, loop (rows 6.4, 6.5, 6.6, 6.7, 6.9)
- [x] **6.4** the loop wired into the pipeline, fire rate zero on the fixtures (row 6.10)
- [x] **6.5** `report.json`, `--report`, and the post-cap policy (rows 6.8, 6.12)
- [x] **6.6** warning codes and the en/de/tr templates (row 6.11)
- [x] **6.7** Playwright DOM checks (rows 6.13, 6.14, 6.15)
- [x] **6.8** the Tier-3 Ace runner and the nightly `ace-a11y` job

**`oc_validate::structural::validate_structural` is the structural validator**, and its report
holds on all ten fixtures: I-7, image parity, the note bijection, resolving hrefs, a sane heading
tree. `crates/openconvert/tests/snapshots/structural__structural_report_per_fixture.snap` is the
measurement; three findings are carried out of it, all three in `docs/DECISIONS_LOG.md`, 2026-09-18:

- **Retention is a flag, not a gate.** Four fixtures land at 0.968–0.973 because furniture removal
  is part of `C_0`, and `validate.min_char_retention = 0.98` is jointly unsatisfiable with the 0.04
  furniture budget. Appendix D's v1.0 retention gate needs Phase 7's strata to be stated at all.
- **The Gopher n-gram statistics cannot gate output text.** `f09` scores `top_3gram` 0.8008 against
  a 0.18 bound, and is correct: its printed contents page is hundreds of `. . .` leaders. The nine
  statistics are recorded; what warns is `DuplicateStats` over emitted **blocks**, at
  `validate.dup_block_frac = 0.02`.
- **The h1-count range is scoped by page count** (`validate.h1_count_min_pages = 20`), so every
  fixture answers `None` and the check is first exercised on the Phase 7 corpus.

**`oc-validate` gained `oc-text` and `oc-core` as dependencies**, which ARCHITECTURE §3.1's table
does not list; the argument is in the same log entry and in `crates/oc-validate/Cargo.toml`.

**`oc_validate::repair` is the loop.** A repair edits the `Document` and the book is re-emitted; it
never patches the zip. All four control rules are in place and each was mutation-tested — deleting
any one of strict decrease, the new-id rule or the hash rule turns exactly one of rows 6.4/6.5/6.6
red. The loop takes an `Emit` trait rather than a concrete emitter, because the emitter passes
EPUBCheck clean on every fixture and therefore cannot be made to oscillate: the cases that matter
only exist against a double. `Emit::apply` is provided (it calls `apply_fix`) so that the control
flow and the table's edits are testable apart.

**The repair table has three `AutoFix` entries, not thirty.** `ACC-001` describes a figure whose
`alt` is empty; `RSC-012` demotes a note reference whose target does not exist to plain text;
`OPF-003` drops a figure nothing references and that carries no caption. All three are Conserving:
`alt` is an attribute, a `noteref` is a link, and an uncaptioned figure carries no characters. Six
more ids are `WarnUser` — understood, and with no fix that would not change the book. Everything
else is unmapped and reported verbatim. The plan's "~30 ids" would have been thirty untested paths
for defects this emitter has never produced.

**The loop is on the real path and fires zero times on all ten fixtures** (row 6.10, A6.2). The loop
owns the emission: `epub_stage` is split into `build` and `epub_check`, because a `convert` that
built the book once for the conservation check and again for the loop would re-encode every image
twice, and PIPELINE §12 budgets one regeneration *per iteration*. `Conversion` now carries `tier1`,
`structural` and `repair` alongside the document and the bytes.

**`validate` and `repair` are declared stages and are in the ledger**, both Conserving. `repair`'s
check is the one that earns its keep: it compares `C` of the document the loop was given against `C`
of the document it settled on with an empty ledger, so a repair that changed one character of the
book fails I-1 and the conversion stops — PIPELINE §12's "none of them may change the character
content of the book", stated as an invariant instead of as a property of three functions. `repair`
is Conserving with an *empty* reason set: PIPELINE §12 words it "except `UserOverride`", and
declaring a reason a Conserving stage may never cite (I-3) would be a contract contradicting itself.
The stage becomes `Budgeted` when the review UI arrives in Phase 12.

**A defect found on the way: the `document` stage's conservation check was never recorded.** It ran
— a violation returns `DocumentError` — but `convert` dropped the `StageCheck`, so the ledger named
seven stages where the pipeline had checked eight, and every phase's "checked after every stage"
claim was one stage short in its evidence. Fixed, and `repair::the_ledger_records_validate_and_repair_as_conserving_stages`
asserts the whole list in order.

**`report.json` is written on every conversion**, versioned `openconvert.report/1`, to
`<output>.report.json` or wherever `--report` says, and **before** the atomic rename so a report
exists even for a conversion whose output could not be placed. It lives in `openconvert::report`
rather than in `oc-core` as the plan's file list has it: assembling it needs `Tier1Report`,
`StructuralReport` and `RepairOutcome`, which are `oc-validate`'s, and `oc-validate` depends on
`oc-core`.

`PROVENANCE` now carries `owner` and `review_by` as well as `source` and `evidence` — "this number
is provisional" is only actionable with "owned by whom, revisit by when" beside it — so
`build.rs` emits a `Provenance` struct instead of a 3-tuple. `convert` times each stage and carries
the producer family and the page-class histogram. The NDJSON `warning` event carries `args` now;
Phase 5 emitted the code with an empty object.

**Row 6.8's cap is demonstrated at two levels and neither is a whole-pipeline run**, because it
cannot be: the emitter passes EPUBCheck clean on all ten fixtures, so a real conversion reports
`Clean` before any repair runs. `repair_loop::the_cap_stops_a_loop_that_would_otherwise_keep_going`
exercises the cap against a scripted emitter, and `report::repair_cap_writes_epub_and_marks_invalid`
asserts the policy — report `invalid`, remaining ids verbatim, the EPUB still there. Forcing three
failing iterations out of a correct emitter would make the assertion about the break.

**Twenty-six warning codes, three locales, and a lint that keeps them total.** `oc-core::warnings`
holds the registry (`codes.rs`: the code and the argument names it carries) and the three template
files; `render` fills `{slot}`s and leaves an unfilled one *visible*, because a warning missing its
number has stopped being a factual claim and a brace is a bug report where a blank is a mystery.

The gate has four parts and each was mutation-tested: every code has a template in every locale, no
locale has a template for a code the registry does not know, every `{slot}` is an argument the code
declares, and the English text uses every argument its code carries. The registry cannot be derived
— `oc-structure` owns `W_TABLE_AS_IMAGE` and `oc-core` cannot depend on it — so
**`xtask ci-lint` holds the registry and the tree in agreement in both directions**: a `W_…`
constant anywhere with no entry fails, and an entry nothing defines fails.

`--locale en|de|tr` is new and is not in §2.1's flag list; §2.2's job spec has the field. Without it
the engine's own localisation would be unreachable from the command line and the three template files
would be readable only by a GUI that arrives in Phase 12. The CLI prints the rendered sentences to
stderr only when stderr is *not* the NDJSON channel.

**The DOM checks run, and the `dom-checks` CI job is on.** 198 assertions across three Chromium
viewports (600×800, 390×844, 1024×768), all green, no screenshots anywhere. WebKit fills the
nightly `webkit-dom` job. Each of the three specs was mutation-tested: a 3000px block trips the
overflow check, two swapped nav entries trip the order check, a `display: none` footnote trips the
note check.

`cargo run -p xtask -- dom-fixtures` converts every fixture through the one pipeline the CLI drives
and unpacks the containers into `target/dom/<fixture>/`, with a manifest holding the **spine** and a
**note-reference inventory** — the two things a browser cannot enumerate from inside one document.
The nav order is read *in the browser*, deliberately: a spec that read one side of the heading-order
comparison out of JSON our own Rust wrote could not fail when the nav itself was wrong.

**A nav entry is not always a heading**, and the first draft of the order spec assumed it was. The
unheaded preamble becomes a front-matter section so nothing is lost (PIPELINE §8), and the nav names
that section, which has no heading in it — f02, f03 and f07 all failed until the claim was restated
over nav *targets*.

Two emitter properties the specs found already true, and worth knowing: `pre { white-space: pre-wrap;
overflow-wrap: break-word }` means a 400-character unbroken line does not overflow a phone, and the
table rules keep a wide table inside its column. The plan named both as the cases row 6.13 would
fail on.

**Tier 3 is wired.** `oc_validate::ace` runs Ace by DAISY and reads its report; the nightly
`ace-a11y` job installs `@daisy/ace` and turns the `ace` cargo feature on. Both halves of
PIPELINE §11's gate are asserted — zero serious violations, **and** every required accessibility
metadata field present — because a book with no `schema:accessMode` passes every structural check
and leaves a screen-reader user unable to decide about it before opening it. `critical` counts as
serious: a gate written against the word alone would pass a book with a critical violation in it.

The metadata half is also asserted **without Node**, on every fixture, by reading the package
document: what the gate really claims about those properties is that the emitter writes them, and
that claim should not be testable only in a nightly job.

**What Phase 5 hands it**, in the order it will be wanted:

1. **`openconvert::convert::convert` is the one pipeline.** It runs every stage, checks each under
   the conservation law, and returns `Conversion { document, built, extracted_images }`. The CLI,
   the tests and (from Phase 12) the desktop app all take that path. `document.ledger` carries
   every stage's `StageCheck`, including `epub`'s.
2. **`oc_validate::validate_tier1` is the issue source the repair loop consumes.** Its `Finding`
   carries an EPUBCheck message id where one exists (`RSC-005`, `RSC-007`, `RSC-012`, `OPF-014`,
   `OPF-003`, `OPF-012`, `OPF-030`, `OPF-060`, `PKG-007`, `PKG-008`, `RSC-002`, `ACC-001`) and an
   `OC-…` id where it does not (`OC-SCRIPT`, `OC-REMOTE`, `OC-ENTITY`, `OC-NOTE-BIJECTION`,
   `OC-IMAGE-PARITY`). `Severity` is already the `(fatal, error, warning)` the repair loop's
   lexicographic measure needs (D13.7).
3. **I-7 is one function call away.** `document.ledger.removed_all()` / `added_all()` are the two
   halves, `ledger.c_0` is the baseline, and `oc_epub::textcontent::body_text` is how `C(EPUB)` is
   measured — by parsing the emitted documents, not by asking the emitter.
4. **The repair-fire rate is a release metric with target zero.** Every repair that fires is an
   emitter bug, so Phase 6 starts from an emitter that EPUBCheck already passes clean; a repair
   that fires on a fixture means something regressed.
5. **`Document.warnings` is where the report's issue list comes from.** Phase 5 added
   `W_EPUB_LARGE`, `W_XHTML_OVERSIZE`, `W_PAGE_BREAK_UNPLACED`, `W_NO_TEXT_EXTRACTED`.
   `BuiltEpub.warnings` carries codes rather than `Warning`s — `oc-epub` does not depend on the
   pipeline — and nothing yet attaches them to the document; that is Phase 6's report to do.
6. **Fixture numbers.** Taken: **f01–f10**, **h01–h29**. Next free: **f11**, **h30**.
   No new fixtures in Phase 5: the emitter's subject is the ten documents that already exist.
7. **Still open, and cheap:** CI's `test` job runs `xtask fixtures` but never `handmade-fixtures`
   or `mutations`, so a builder change that no longer reproduces the committed fixtures is not
   caught. A `--check` mode on those two tasks would close it.

## Notes

Carried forward, in the order a fresh session needs them:

- **`furniture` recovers no folio from a book that changes numbering system.** `f09` paginates
  `i, ii` then `1, 2, 3`; digit masking puts the three arabic folios in one group covering 3 of 5
  pages, a repetition ratio of 0.6, inside the grey zone where the detector abstains. The folios
  stay in the flow as one-character paragraphs and every `PageRef.label` is `None` — which also
  removes the arabic-1 reset PIPELINE §9 step 1 calls a hard boundary signal. The fix is a change
  to how a folio group is scoped (per numbering system, or per pagination run) and wants the
  Phase 7 corpus to choose between them. `docs/DECISIONS_LOG.md`, 2026-09-14.
- **Two of four real books outside the corpus are refused by the conservation law**, both in
  `structure`, and both are Phase 7's to calibrate rather than Phase 5's to guess at:
  - *AI Engineering* (O'Reilly, ~500 pp): `structure` emits 21 526 characters twice. The ruled-
    table detector finds **365 tables** in a book that has perhaps twenty — it fires on figure
    boxes and code blocks — and `tables.consumed` does not cover every block whose text it
    claimed, so the same lines are in a table's cells *and* in the flow. Attributed by source,
    every duplicate but three involves a table (`list+table` 84, `caption+table` 47,
    `heading+table` 16). `table.{min_row_rules, min_column_rules, grid_snap_pt, rule_overlap_min}`
    are all `provisional` and have never met a real book.
  - *Aus dem Leben eines Taugenichts* (Project Gutenberg): 66 730 characters **lost** with no
    ledger entry — the other direction, and a different bug.
  - *Tschick* and *O Crime do Padre Amaro* convert clean, and EPUBCheck reports 0 errors and
    0 warnings on both. The law refusing two books rather than shipping duplicated or missing
    paragraphs is it working.
- **A fallback table is emitted as a grid, not as an image plus `<details>`.** PIPELINE §8.7 wants
  the image; rasterising a vector region needs a page renderer in `oc-pdf` that does not exist.
  No text is lost either way, and `El::details` is written and tested for when it does.
- **A tightly set ruled table falls back to an image.** When a cell gutter is narrower than
  `text.line_split_gap_em`, `words` keeps two cells in one run and a `Run` carries a box and its
  text but not its glyphs' positions, so nothing in `structure` can split it. The long-term fix is
  to split runs at vertical rules in `layout`, which needs the rules to reach `layout`.
- **A contents page with no drawn leader is not parsed.** `"Preface    i"` reaches the parser as
  `"Preface i"` — the gap is geometry and a line's text is not.
- **`Note.body` is one paragraph.** A note that runs to several paragraphs is one paragraph here.
- **`RunId` is page-local**, whatever IR_SKETCH calls it. Every map from a run is keyed on
  `(page, RunId)`; `oc_structure::build::NoteRefRuns` names the pair once.
- **Language detection is not wired into the driver.** `convert` uses the configured tag and falls
  back to `LangTag::UND`. `whatlang` is a Phase 2 capability that the stage driver never calls.
- **EPUBCheck and its corpus are fetched, never committed**: `cargo run -p xtask -- fetch-epubcheck`
  and `fetch-epubcheck-corpus` put them under `vendor/`, which `.gitignore` covers.
- Local tool versions: rustc 1.98.1, cargo-nextest 0.9.143, cargo-deny 0.20.2, EPUBCheck 5.3.0,
  Temurin-compatible JVM 23 locally / Temurin 21 in CI, Python 3.13.7 + uv 0.11.28.
- **The Python side runs out of `eval/.venv`** (`uv venv eval/.venv && uv pip install --python
  eval/.venv -e "eval[dev]"`). On the maintainer's Windows box `python3` exists only because a
  copy of `python.exe` was placed beside it under that name; CI's Linux runners have the real one.

## Phase 7 — what it has built so far

- **`oc_eval.corpus.download`** fetches mirror-first, verifies the manifest's sha256 before the
  bytes are placed, and deletes a file that fails rather than quarantining it. The opener is a
  parameter, so no test needs a connection; the default refuses any URL that is not https.
- **`oc_eval.corpus.manifest`** is the vocabulary — licence allowlist and blocklist markers, the
  nine producer strata plus `page-level-layout`, and the document-vs-page unit.
  **`oc_eval.corpus.lint`** is fourteen rules over it, one stable slug each, and the slugs are
  the contract the tests assert on.
- **`oc_eval.thresholds`** reads `thresholds.toml`, so the corpus gates are the same numbers in
  Python and in Rust rather than two that agree today.
- Two rules needed a judgement the plan does not make. `tagged-share-off-target` is skipped when
  the synthetic bucket is too small to land within five points of 0.126 at all — a five-file
  bucket can be 0.0 or 0.2 tagged and nothing between, and reporting that is reporting
  arithmetic. `is_page_level` deliberately does not trust the `unit` field alone: a DocLayNet
  entry is page-level whether or not it says so, and `page-unit-undeclared` separately requires
  it to say so, which is what closes §7.6's per-page shortcut.
- **`mypy` is clean on everything Phase 7 has written** and reports 13 errors in two Phase 2
  files (`generate/scan_sim.py`, `train/hyphen_clf.py`). Item 7.10 owns them, because that is
  where `mypy` becomes a CI gate.

### Item 7.3 — how the corpus was assembled

- **Five adapters, each a pure parser plus a thin fetch loop.** OAPEN's DSpace
  `/rest/filtered-items` filters on `dc.rights.uri` and `dc.language`, so the CC-BY subset and
  the German slice are selected by the catalogue rather than by downloading and hoping.
  Internet Archive filters on `licenseurl` and then picks the scan out of an item's files,
  avoiding the `_text.pdf` and `_djvu.pdf` sidecars. arXiv reads OAI-PMH `arXivRaw`, because
  the Atom search API reports no licence at all and a paper with none is under arXiv's own
  distribution licence, not ours. US-Gov is NASA's NTRS, admitting only
  `copyright.determinationType == "GOV_PUBLIC_USE_PERMITTED"`.
- **The Turkish slice was rewritten mid-item.** Reading the licence off a DergiPark article
  page admitted one article in ten — most DergiPark journals are CC BY-NC-ND. Selecting
  **journals** from DOAJ by their curated `license.type` and then taking a few articles from
  each admits at the journal's rate instead, and DOAJ's `fulltext` link for those journals is
  the direct PDF, so nothing scrapes a page any more.
- **Nothing enters the manifest on a promise.** Every entry's `sha256`, `pages`, `tagged` and
  `producer_raw` come from opening the file that actually arrived. 45 candidates were rejected
  with a named reason across the four runs — duplicates, landing pages that were not PDFs,
  files over the 40 MB per-file budget, documents under four pages.
- **A top-up harvest needed a fix to work at all.** A source adapter re-walks its catalogue
  from the start, so `usgov=2` against thirteen NASA reports already held yielded thirteen
  duplicates and nothing else. `admit(want=…)` caps what one call admits and `ask_for` clears
  the held count, so the generator is drawn further down the catalogue and stops as soon as it
  has enough.

### Item 7.4 — the mutation catalogue

Ten recipes in `oc-testkit::mutate`, listed as data in `xtask::mutations::catalogue()` so that
row 7.6's "every recipe" has one place to be asked. Five are new: `strip_structtree`,
`double_draw`, `ocr_sandwich`, `jitter_spacing`, `damage_xref`.

Each recipe declares two things the tests hold it to.

- **`Reproducibility`.** Running `xtask mutations` twice showed the three encrypted mutants
  rewritten with different bytes every time — 902 bytes on one run, 903 on the next — because
  AES-128 draws a fresh initialisation vector per string and stream. Row 7.6's byte-for-byte
  assertion holds for the seven `Deterministic` recipes; the three `Randomised` ones are
  asserted on the property that makes byte-equality impossible, that two applications differ.
  `xtask mutations` now leaves an existing randomised mutant alone, because regenerating it
  replaced a regression artefact with noise.
- **`Effect`** — what the recipe does to the characters on the page, checked through PDFium
  against the parent. `Preserves` (cropbox offset, struct-tree strip, jitter, damaged xref),
  `Duplicates` (double draw, OCR sandwich — exactly twice the characters, the original first),
  `BreaksTextMapping` (ToUnicode strip), `Unopenable` (encrypted with a user password).
  Flipping one declared effect turns the test red, which is how it is known not to be vacuous.

**Type 3 re-encoding is the one item of PHASE 7 §4's list that is not done.** Re-encoding an
embedded font as Type 3 while keeping its outlines needs a glyph-outline extractor `lopdf` does
not have, and a Type 3 font whose CharProcs draw rectangles would change what the page looks
like rather than only how it is encoded. It belongs with the handmade fixtures — a small PDF
authored as Type 3 with a `/ToUnicode` map — and is a gap, not a mutation.
`docs/DECISIONS_LOG.md`, 2026-09-20.

### Item 7.5 — ground truth

Three sources, one type. `oc_eval.ground_truth.schema.GroundTruth` is TEST_CORPUS §5.1's shape
— headings, paragraphs, footnote pairs, figures — whatever produced it, so the scorer has one
thing to compare against and a fourth source would not touch it.

**The assertions are the engine's own vocabulary.** `to_assertions` emits `heading_tree`,
`text_present`, `block_count`, `image_count`, `note_bijection`, `lang_tag` and `text_order` —
the same kinds `oc_testkit::assertions` reads and the committed `.assert.json` fixtures are
written in — so one runner checks a generated expectation and a hand-written one.
`test_every_generated_assertion_kind_is_one_the_engine_knows` reads the Rust enum rather than
keeping a second copy of the list, the same trick `xtask ci-lint` uses for the warning registry.

**A ground truth never invents what its source does not say.** A tagged PDF's `/H1` points at
marked content, not at characters, so a struct-tree heading has a level and no text — and
`to_assertions` emits no `heading_tree` or `heading_level` for it, because asserting on an
empty string would score every document as wrong. The same rule drops a noteref whose endnote
is in a file that was not read, and withholds `note_bijection` when the notes do not pair.

- `from_xhtml` reads Standard Ebooks markup. Heading levels come from `<section>` nesting, not
  from the tag number: SE writes a chapter title as `<h2>` because the book's `<h1>` is its
  title page, and comparing `h2` against `h1` would score a correct conversion as wrong.
- `from_structtree` reads a tagged PDF's own tree. On the tagged fixture it finds one `/H1`
  and four `/P`, which is the document.
- `from_latex` reads arXiv sectioning, and `looks_parseable` is §5.2's "curated
  parses-cleanly subset" as a predicate — a paper that pulls its sections in through `\input`
  is refused **before** it is scored against rather than after it has quietly scored zero.

`corpus/gt/se_sample/` is a small SE-shaped book written for this repository: a chapter with
two heading levels, four paragraphs, two noteref/endnote pairs and a captioned figure.

### Item 7.6 — the metric suite

Eight modules under `oc_eval.metrics`, TEST_STRATEGY §8.1's table one function at a time:
`cer` (text NED after D13.4's normalisation), `reading_order` (edit similarity plus Kendall
tau), `toc_f1` (heading P/R/F1 and outline edit distance), `footnotes`, `images`, `teds`
(TEDS and TEDS-S), `prf` (the shape the four set-metrics share) and `report`.

Two decisions carry the phase's weight.

- **A pass rate is an interval, not a number.** 94 of 100 and 940 of 1000 are the same rate
  and not the same evidence. `assertions.PassRate.interval()` is Wilson's, pinned against the
  published value for 95/100 — (0.8882, 0.9785) — because the normal approximation is wrong
  exactly where this gate lives, at p near 1. The gate is last-green minus the half-width, and
  a suite below `eval.assertion_min_instances` **fails and says why** rather than passing on
  an interval wide enough to admit any regression.
- **There is no aggregate row and there never will be.** `report.build` emits `per_file`,
  `per_stratum` and `ours_vs_real`, and a test asserts no `aggregate` key appears — an average
  across strata is precisely the number that lets a synthetic win mask a real-book regression,
  which is what D18 exists to prevent. A report with no real strata states no gap rather than
  inventing one.

The metrics normalise before they measure. `cer.ned("ﬁre", "fire")` is 0.0 and
`cer.ned("pipe-
line", "pipeline")` is 0.0, because the pipeline is *supposed* to fold those
(D13.4's `N`) and a metric that charges for them scores a correct conversion as wrong — R9
§A.10's gap in Nougat's metric. What is not folded is anything the pipeline may not change: a
hyphen inside a line is content and still costs.

Two new thresholds: `eval.assertion_confidence` (0.95, binary — the plan's level) and
`eval.assertion_min_instances` (30, provisional — where the Wilson half-width at p ≈ 0.95
falls under eight points).

### Item 7.7 — the trend

`oc_eval.trend` keeps the `ours(*)`-versus-real gap in `eval/out/trend.json`, in the
repository, because TEST_STRATEGY §8 wants a regression to be a diff against the
immediately-prior committed baseline. One entry per commit: re-running the nightly on an
unchanged tree corrects the record rather than doubling it.

The point is the **direction**, not the number. A gap of 0.06 is fine or alarming depending on
whether last week's was 0.05 or 0.09, so `widening()` reports first-to-last and returns None
when the history holds fewer than two runs that stated a gap at all — a run with no real strata
states no gap, and is not evidence about widening either. `plot()` draws it to a PNG under a
headless backend.

### Item 7.8 — the holdout refusal

TEST_CORPUS §7.1(c) as a mechanism rather than a sentence. Every fit goes through
`calibrate.fit`, which loads the manifest and raises `HoldoutLeak` on any id marked `holdout`.
Two details make it hold:

- **an unknown id is refused too** (`UnknownFile`). An id the manifest cannot account for is
  not evidence that it is not holdout, and "I could not check" must not read as "it is fine";
- **one offending file stops the whole fit.** Dropping it and carrying on would produce a
  number that looks fitted on what was asked for and was not.

`risk_coverage` is where an escalation threshold actually comes from (D17): sort by
confidence, and report the widest coverage whose risk is still under the target — returning
None when nothing meets it, rather than the best available dressed up as a hit. `reliability`
is the ECE and the diagram rows behind it.

### Item 7.9 — the performance budget

**Measured: 0.0217 s/page over 300 pages**, against D13.11's 0.5 — a 23x margin, in an
unoptimised `test` profile on the maintainer's machine. The gate was confirmed to fail when the
budget was lowered below the measurement, so it is not vacuous. Machine L's number will differ;
this is a floor on the headroom, not the reference measurement.

`oc_testkit::handmade::reference_book(pages)` builds the input rather than committing it: three
hundred pages of prose is a megabyte of fixture nobody would regenerate or review. It is
deterministic, and it carries what makes a book *expensive* rather than merely long — a running
head and a folio on every page for furniture detection to find, a chapter opening every twenty
pages, body lines at a real leading.

The split: the **arithmetic** runs on every PR — that row 7.12's five stage budgets sum to
`perf.seconds_per_page_max`, that each is a positive share of it, that the reference book is the
300 pages D13.11 states the budget for — because that is where the mistake that actually happens
gets caught, a stage quietly given room the whole does not have. The **timed** assertions are
behind the `bench` cargo feature, which the nightly job turns on: a wall-clock assertion on a
shared CI runner measures the runner, and a gate that fails for that reason is one people learn
to re-run until it passes.

`oc_eval.bench.peak_rss` measures the **whole process tree** — `/proc`'s `VmHWM` on Linux, a Job
Object on Windows, `psutil` polling otherwise — because D13.11's 500 MB is for the converter and
whatever it spawns, and a figure that counts only the parent stops being true the moment Phase
9's sidecar exists. It says which mechanism it used and whether the answer is exact, and a
sampled floor is printed as a floor.

Six new thresholds: the five per-stage budgets and `perf.bench_reference_pages`.

## Blocked

_(empty — Q1 resolved 2026-09-09; see `docs/DECISIONS_LOG.md`)_

## Phase 7 — Definition of Done

`IMPLEMENTATION_PLAN.md` §0.3, row by row. Checked on this machine unless the row says otherwise.

| Row | State |
|---|---|
| Every named test exists and passes | **Yes.** All 14 of PHASE 7's rows, plus ~180 additions. Rows 7.11 and 7.12 are behind the `bench` cargo feature and pass with it on. |
| `cargo nextest run --workspace` green | **Yes**, 466 tests. |
| Green on Linux/macOS/Windows CI | **Not verifiable here.** Windows only. |
| clippy `-D warnings` clean | **Yes**, workspace, all targets, all features. |
| `cargo fmt --check` clean | **Yes.** |
| `cargo deny check` clean | **Yes** — no new Rust dependency; `criterion` was already in the workspace manifest. |
| `cargo xtask thresholds-lint` clean | **Yes.** Eight thresholds added, each with source, evidence, owner and an unexpired `review_by`. |
| Every Given/When/Then demonstrated | **A7.1, A7.1b, A7.2 and A7.3 yes** (below). **A7.4 partially**: the gate is written and tested, and there is no "last green" until the nightly has run once. |
| `docs/CHANGELOG.md` entry | **Yes.** |
| No `TODO`/`FIXME` without an issue number | **Yes**, `xtask ci-lint` clean. |

### Acceptance criteria

- **A7.1** — `oc-eval corpus lint` is clean. `ours(*)` is 0.103 against the 0.40 cap; the
  holdout is 104 **documents**, counted with page-level entries excluded.
- **A7.1b** — the composition meets §7.6's target as written: OAPEN 44 (~40), Internet Archive
  20 (~20), arXiv 15 (~15), US-Gov 15 (~15), DergiPark 10 (~10), and the German (34) and
  Turkish (10) slices are non-empty.
- **A7.2** — `report.build` emits `per_file`, `per_stratum` and `ours_vs_real`, and
  `test_per_stratum_scores_are_reported_separately` asserts no `aggregate` key exists.
- **A7.3** — **0.0217 s/page over 300 pages**, unoptimised `test` profile, against 0.5. The
  gate fails when the budget is lowered below the measurement. Reference machine L will differ.
- **A7.4** — the gate is `last green - margin` where the margin is the Wilson half-width, and
  it refuses to gate at all below `eval.assertion_min_instances`. The first nightly records the
  baseline it will compare against.

### What Phase 7 was asked to settle, and did not

The seven items the phase was carrying are not all closed, and it is worth saying which.

1. **`validate.min_char_retention` is still not a gate.** The corpus now exists to choose
   between the two candidate definitions, and nothing has run against it yet. Still open.
2. **`validate.h1_count_min_pages = 20`** is first exercisable now — 104 real documents,
   most well over twenty pages — but the first exercise is the nightly's, not this branch's.
3. **`validate.dup_block_frac = 0.02` has still met no real book.** Same reason.
4. **The two real books the conservation law refuses** have not been re-run. They are in
   `example_pdfs/`, which is not corpus, and the table thresholds are still `provisional`.
5. **A2.3, A3.2, A3.3 and A4.3** are still partial. The corpus exists; the gold data for them
   does not, and `corpus/gt/` holds one hand-written sample rather than §5.5's ~50-file set.
6. **The benchmark harness is done**, and A1.6 is a measurement on this machine rather than
   on reference machine L.
7. **Repair fires per id per stratum** — `oc-eval run` records `repairs_fired` per file and
   the report groups per stratum, so RT A10.4 is answerable on the first nightly.

## Phase 6 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-18, every gate run for real. **Two rows
cannot be verified on a single machine** and are named as such rather than counted as passes.

1. **Every named test exists and passes** — all sixteen rows of the Phase 6 table (6.1–6.16), each
   run individually by name, plus about sixty additions. `docs/TEST_MATRIX.md` lists every one and
   the CI job that runs it. Rows 6.13–6.15 are the Playwright specs (44 + 20 + 2 assertions at one
   viewport, 198 across three); row 6.25 is behind the `ace` cargo feature; neither is `#[ignore]`d,
   which `xtask ci-lint` enforces.
2. **`cargo nextest run --workspace` green on three OSes** — **cashed.** CI run 35462059355 on `main`:
   `test (ubuntu-latest)`, `test (macos-latest)` and `test (windows-latest)` all green, alongside
   every other job. 456 locally. It was **not** green on the first attempt: macOS failed
   `golden_epub_bytes_f01` and Ubuntu ran out of disk, both for real reasons, both fixed.
3. **`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`** — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny --all-features check`** — advisories, bans, licences, sources ok.
6. **`xtask thresholds-lint`** — clean; four new entries, each with the five keys. **`ci-lint`** —
   clean, including its new rule that the warning registry and the tree agree in both directions.
7. **Every acceptance criterion demonstrated:**
   - **A6.1** (I-7 holds) — `i7_holds_end_to_end_on_all_fixtures` over all ten, measured over the
     **archive** through the package document's spine, plus
     `the_archive_and_the_emitter_agree_about_the_text` for the two measurements agreeing. "Every
     corpus file" is Phase 7's corpus; the same scope limit as A2.3 and A3.2, for the same reason.
   - **A6.2** (zero repairs fire) — `repair_fire_rate_is_zero_on_corpus`, against
     `repair.corpus_fire_rate_max = 0`. Every fixture reports `RepairStatus::Clean`.
   - **A6.3** (≤ 3 iterations, strictly decreasing `M`, or a named status) — the four control rules,
     each **mutation-tested**: deleting strict decrease, the new-id rule or the hash rule turns
     exactly one of rows 6.4/6.5/6.6 red, and nothing else.
   - **A6.4** (the report's contents) — `the_report_carries_every_part_the_plan_names` asserts each
     part the plan's detail 6 names, by name; `report_schema_is_valid_and_snapshotted` snapshots
     `f07` with the timings redacted.
   - **A6.5** (3 viewports, no overflow, nav/DOM agreement, noterefs resolve) — **cashed.** The
     `dom-checks` job passed on its first real run in CI 35462059355, and nightly `webkit-dom` passed
     too, so the assertions hold in both engines. 198 Chromium assertions, each spec
     mutation-tested (a 3000px block, two swapped nav entries, a `display: none` footnote).
8. **`docs/CHANGELOG.md`** — Phase 6 entry written.
9. **No `TODO`/`FIXME` without an issue number, no `#[ignore]`** — `xtask ci-lint` clean; zero
   `test.skip` or `.only` in the Playwright specs either.

**One file of the plan's Files list is elsewhere**, with the reason recorded: `report.rs` is in
`openconvert`, not `oc-core`. Assembling the report needs `Tier1Report`, `StructuralReport` and
`RepairOutcome`, which are `oc-validate`'s, and `oc-validate` depends on `oc-core` — the plan's
placement is a cycle.

**One deliverable is deliberately smaller than the plan asks.** The repair table has three `AutoFix`
entries where the plan says "the ~30 ids our own generator can plausibly trigger". This emitter
triggers none of them: EPUBCheck reports zero errors on all ten fixtures. Thirty speculative repairs
would be thirty untested paths, against the one gate (A6.2) that says every repair firing is an
emitter bug.

### What Phase 6 found that Phase 5 did not

- **The `document` stage's conservation check was never recorded in the ledger.** It ran — a
  violation returns `DocumentError` — but `convert` dropped the `StageCheck`, so every phase's
  "I-1 … I-4 were checked after every stage" rested on a record naming seven stages where the
  pipeline had checked eight.
- **`validate.min_char_retention` and `conservation.budget.furniture` are jointly unsatisfiable.**
  Four fixtures retain 0.968–0.973, all of it ledgered furniture inside its budget.
- **The Gopher n-gram statistics cannot gate output text.** `f09` scores `top_3gram` 0.8008 against
  a 0.18 bound and is correct: its contents page is hundreds of `. . .` leaders.

### What CI found that Phase 6's own testing did not

Seven defects, none a flake, none findable on one machine. The full argument for each is in
`docs/DECISIONS_LOG.md`, 2026-09-19; in one line each:

1. **The container's bytes differed on Windows.** `zip` fills the "version made by" host byte from
   the building platform; the field is fixed-width, so the EPUB came out the same length with
   different bytes. `zip.rs` already *claimed* the field was pinned.
2. **Ubuntu ran out of disk mid-link.** Phase 6's eight new integration-test binaries;
   `debug = "line-tables-only"` cut the workspace's test executables from 5.7 GB to 488 MB.
3. **`epub-pagesource`, serious.** A book publishing page numbers did not say where they came from.
4. **`metadata-accessmodesufficient`.** The condition was inverted — textual sufficiency claimed for
   books that *had* images and withheld from books that were nothing but text.
5. **`epub-type-has-matching-role`**, on every content document: no `role="doc-chapter"`.
6. **The Ace runner read `data.metadata`**, a key Ace does not write, so the metadata half of its own
   gate would have reported everything missing on every book. Its unit fixture had been composed from
   the documentation by the same hand as the parser.
7. **`xtask fetch-epubcheck` unpacked a zip with `tar`.** Works on Windows, where `tar` is
   libarchive; GNU tar refuses it. Phase 5 code, and `epubcheck` and `tier1-parity` had never once
   run in CI because `needs: test` had never passed.

Nightly is green too (run 35462081049): `webkit-dom`, and `ace-a11y` once Ace's Electron was given
a root-owned setuid `chrome-sandbox` and an Xvfb display.

## Phase 5 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-14. **One row cannot be verified on a
single machine** and is named as such rather than counted as a pass.

1. **Every named test exists and passes** — all twenty rows of the Phase 5 table (5.1–5.20), plus
   about sixty additions. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
   Row 5.17 is behind the `epubcheck` cargo feature and row 5.5 is a CI job; neither is
   `#[ignore]`d, which `xtask ci-lint` enforces.
2. **`cargo nextest run --workspace` green on three OSes** — green locally on Windows. The Linux
   and macOS legs are the `test` matrix job and are unverified until CI runs.
3. **`cargo clippy --workspace --all-targets -- -D warnings`** — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — clean (advisories, bans, licences, sources).
6. **`cargo xtask thresholds-lint`** — clean; four new entries, each with the five keys.
7. **Every acceptance criterion demonstrated:**
   - **A5.1** (every fixture, 0 EPUBCheck errors) — **met, and measured**: EPUBCheck 5.3.0 reports
     0 errors *and 0 warnings* on all ten fixtures. Row 5.17 is the standing gate.
   - **A5.2** (the same input twice is byte-identical modulo `dcterms:modified`) — row 5.4 over all
     ten fixtures, and `cli::convert_writes_a_valid_container_and_leaves_no_temporary` through the
     binary.
   - **A5.3** (identical sha256 on ubuntu/macos/windows) — **partial, and unverifiable here**: the
     `epub-bytes` matrix job and the `epub_is_byte_identical_across_os` job are written, and one
     machine cannot run them. Everything they depend on — pure-Rust codecs, a fixed JPEG quality, a
     fixed resampling filter, sorted entries, fixed timestamps, `--modified` — is in place and
     tested on this machine.
   - **A5.4** (Tier 1: bijection, page-list, alt, no-script) — `tier1_passes_on_every_fixture`,
     plus one test per check against a container broken in exactly that way.
   - **A5.5** (a footnote inside a paragraph does not compile) — `phrasing_cannot_contain_figure`
     is the same claim about the same trait bound; `El<Phrasing>` has neither `figure` nor
     `aside_footnote`, both being `FlowContext` methods.
   - **A5.6** (per-message-id parity recorded and non-decreasing) — `docs/TIER1_PARITY.md` is
     generated by `xtask epubcheck-parity` over EPUBCheck's own corpus, and `--check` is the CI
     gate. Expanded publications are zipped by this project's writer on the way in, so an
     OCF-level defect in one of those cases is repaired before Tier 1 sees it; the 25 packaged
     `.epub` files are the ones whose container bytes are measured.
8. **`docs/CHANGELOG.md`** — Phase 5 entry written.
9. **No `TODO`/`FIXME` without an issue number** — `xtask ci-lint` clean.

### What EPUBCheck found that the tests did not

Two real defects, both fixed, both now with a Tier-1 check of their own:

- Image `src` was written package-root-relative from a document in `text/`, so every figure
  resolved to `text/images/…` and was missing (`RSC-007`). Tier 1 checked fragments and not
  resources; it checks both now.
- A document that yielded no text produced an empty spine, an empty nav `<ol>` and an empty
  `navMap` — three `RSC-005`s and not a book. A book with no text now carries its pages as
  figures (PIPELINE §10).

That is the whole argument for D6's Tier 2 being a hard gate rather than a nice-to-have.

## Phase 4 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-14. **One row is partial** and it is
blocked on the corpus, which is Phase 7's; it is marked as such rather than counted as a pass.

1. **Every named test exists and passes** — all twenty-one rows of the Phase 4 table (4.1–4.21),
   plus about fifty additions. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
   The plan's fixture numbers were all spent in Phases 2 and 3, so `f06`–`f08` there are
   `f08`–`f10` here and `h15`–`h20` are `h24`–`h29`; the test *names* are unchanged.
2. **`cargo nextest run --workspace`** — 307 passed, 0 skipped, 0 ignored, locally. The three-OS
   claim is CI's and is made when this branch merges, as Phases 2 and 3 cashed theirs.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources ok; `deny.tools.toml` ok. `uuid`
   (MIT OR Apache-2.0) is the only new shipped dependency, and the plan names it.
6. **`cargo run -p xtask -- thresholds-lint`** — clean. **`ci-lint`** — clean.
7. **Acceptance criteria A4.1–A4.6.**
   - **A4.1** (outline → headings 1:1) — `outline_is_used_as_heading_ground_truth` on `f09`, in
     both directions: every outline entry binds and no heading is emitted that the outline did
     not name.
   - **A4.2** (noteref↔footnote bijection total) — `footnote_marker_body_bijection` on `f08`
     (`match_rate == 1.0`, no anchor shared) and `footnote_symbol_cycle_resets_per_page` on `h24`.
   - **A4.3** (heading F1 ≥ 0.75 against ground truth on the corpus) — **PARTIAL.** There is no
     corpus and no heading ground truth to score against; both arrive in Phase 7. What exists is
     the *exactness* of the two fast paths on `f09` (outline and contents page, 7/7 each), size
     rank on `f10` (4/4), and `heading_tree_has_no_level_skips` over eight fixtures under all
     three sources. The same shape of partial as Phase 2's A2.3 and Phase 3's A3.2, and the same
     cause.
   - **A4.4** (cell multiset equals source, or the table becomes an image) — enforced as the gate
     itself in `tables::build_table`, demonstrated by `ruled_table_becomes_html_table` on `f10`
     for the markup path and `borderless_table_falls_back_to_image_with_details` on `h26` for the
     other one.
   - **A4.5** (ledger empty) — `structure_stage_is_conserving` over nine documents, through the
     real `check_invariants` with `StageKind::Conserving`.
   - **A4.6** (identical `dc:identifier` across reconversions) —
     `identifier_is_stable_across_reconversions`, as a unit test on the function and end to end on
     `h28`, including that the filename does not enter it and the hash does.
8. **`docs/CHANGELOG.md`** — Phase 4 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

**Three defects in earlier phases were found and fixed here**, each recorded in
`docs/DECISIONS_LOG.md`:

- `read_outline` expanded every `/Next` chain at every node, so a five-entry chain came back with
  thirty-two entries and `f09`'s seven headings with thirty-four. The outline is heading ground
  truth, so every duplicate would have become a heading. `h13` hid it (its chains are two long)
  and `f01` hid it (one bookmark).
- Docstrum merged every heading into the paragraph beneath it, and `paragraphs` merged them back
  when the barrier was added to `layout` alone.
- A superscript set with an OpenType `sups` glyph was read as ordinary text, so footnote markers
  were swallowed into the middle of body runs.

**Known gaps carried out of the phase**, each with a named cause, are in the Notes below.

## Phase 3 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-13. **Two rows are partial and both are
blocked on work outside this phase**; they are marked as such rather than counted as passes.

1. **Every named test exists and passes** — all seventeen rows of the Phase 3 table (3.1–3.17), plus
   about sixty additions. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it. Three of the
   plan's fixture numbers were already spent, so test 3.5 uses `h22_false_gutter`, test 3.7 uses
   `h23_paragraph_across_pages` and test 3.9 uses `f06_hyphenation_de`; the test *names* are unchanged.
2. **`cargo nextest run --workspace`** — 240 passed, 0 skipped, 0 ignored, locally. The three-OS claim
   is CI's and is made when this branch merges; Phase 2's was cashed the same way.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources ok; `deny.tools.toml` ok. Both configs
   now ban `hyphenation` (VD-b).
6. **`cargo run -p xtask -- thresholds-lint`** — clean. **`ci-lint`** — clean: no `#[ignore]`, no
   unnumbered `TODO`.
7. **Acceptance criteria A3.1–A3.5.**
   - **A3.1** (`f02`, left column before right) — `two_column_reading_order_is_left_then_right`.
     Cashed properly only because the fixture was rewritten: it had never had two columns.
   - **A3.2** (100 % of gold block orders on any Manhattan corpus file) — **PARTIAL.** There is no
     corpus and no gold block order to reproduce; both arrive in Phase 7. What exists is the order
     itself, on `f01`, `f02` and `h22`, plus `prop_single_column_order_is_monotone_in_y` over generated
     pages and `prop_page_permutation_metamorphic` over three documents. The same shape of partial as
     Phase 2's A2.3, and the same cause.
   - **A3.3** (keep-hyphen recall ≥ 0.80 on the holdout, for the DE fixture) — **PARTIAL**, and split in
     two. The recall is measured and passes — 0.912 over 239 held-out keeps
     (`hyphen_classifier_keep_recall_on_holdout`) — but the holdout is **English**, because D15 has no
     German source that may be redistributed and so there is no German training data. The German fixture
     itself is checked in both directions by `dehyphenate_keeps_german_real_hyphen`, on the rule that
     needs no lexicon. Closing this row properly needs the DE list, which is the open D15 question from
     item 2.9.
   - **A3.4** (layout ledger empty) — `layout_stage_is_conserving`, and
     `layout_stage_conservation_violation_errors` for the other direction.
   - **A3.5** (I-5: exactly one hyphen, nothing else) — `dehyphenate_i5_removes_exactly_one_hyphen`
     (5,000 generated joins) for the operation, `check_invariants` for the ledger, and
     `dehyphenation_is_ledgered_one_hyphen_at_a_time` end to end on `h23`.
8. **`docs/CHANGELOG.md`** — Phase 3 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

**VD-b closed** (2026-09-13): the `hyphenation` crate ships the `hyph-utf8` pattern files with their
licence headers stripped and disclaims them; Turkish is LPPL-1.0+ upstream and the compiled dictionaries
fold in GPL/LGPL/MPL extended data. It is banned in both `deny` configs. v1 needs no patterns.

**One file from the plan's Files list is deliberately absent:** `f05_verse_and_quote.typ` (which would be
`f07` here). No Phase 3 test names it — verse and block quotes are PIPELINE §8.6, and their tests are
Phase 4's. It is written when the test that needs it is.

**Known gaps carried out of the phase**, each with a named cause:

- `f01`'s committed `pipeline` assertion is not met. The classifier reads `pipe-line` as a real compound,
  which it was in the nineteenth-century register its training corpus is written in. Fails in the safe
  direction; the fix is a modern corpus (Phase 7).
- One block of `f02` is flagged low-confidence: the cover cuts a paragraph's last line off because a line
  with no ascenders has a shorter inked box. Over-flagging a confidence signal is the safe direction, and
  the flag changes no segmentation.
- A `Dehyphenate` budget is a fraction, so a document of a hundred characters breaches it on one
  legitimate hyphen. A floor on the denominator belongs with the rest of the breach policy, in Phase 6.

## Phase 2 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-13:

1. **Every named test exists and passes** — all twenty-two rows of the Phase 2 table (2.1–2.22),
   plus about forty additions, each of which exists because something was measured and was not
   what the plan assumed. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
2. **`cargo nextest run --workspace`** — 156 passed, 0 skipped, 0 ignored, **on all three
   operating systems**. CI run 34758095844 on `main`, 2026-09-13: `test (ubuntu-latest)`,
   `test (macos-latest)` and `test (windows-latest)` all green, alongside `lint`, `deny`,
   `desktop`, `no-network`, `poppler-oracle` and `ui`. This is the first time that claim has
   been true rather than deferred — see the Notes below.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources ok; `deny.tools.toml` ok.
6. **`cargo run -p xtask -- thresholds-lint`** — clean.
7. **Acceptance criteria A2.1–A2.6.**
   - A2.1 (I-1…I-4 hold, a violation is fatal) by `conservation_i1_holds_across_text_and_furniture`
     over `f01`/`f02` through the real checker, `conservation_i1_holds_over_generated_documents`
     over 200 generated documents, and `a_budget_breach_stops_the_stage`.
   - A2.2 (`"The Test Book"` and the page numbers absent from flow, present in the ledger) by
     tests 2.10 and 2.11.
   - A2.3 (furniture ≤ 4 % of `|C_0|`) enforced by `check_invariants` on every stage run and
     demonstrated on `f01` and `f02`; **"any corpus file" is Phase 7's corpus**, which does not
     exist yet, so this is demonstrated on the fixtures rather than at the stated scope.
   - A2.4 (10 000 random strings: idempotent, NFC, no NFKC, no case folding) by
     `normalize_is_idempotent` (10 000 cases), `normalize_never_applies_nfkc`,
     `normalize_composes_to_nfc` and `text_is_never_case_folded_in_output`.
   - A2.5 (EN/DE/TR `dc:language`) by test 2.19 over `f01`, `f04` and `f05`.
   - A2.6 (Turkish folding, emitted text unchanged) by tests 2.6 and 2.7.
8. **`docs/CHANGELOG.md`** — Phase 2 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

**One deliverable is deliberately partial, with the maintainer's agreement.** PLAN Phase 2
detail 5 asks for EN/DE/TR word-frequency lists. English ships (20 000 words from twelve CC0
Standard Ebooks, with a source manifest). German and Turkish do not: the plan names DTA plain
text and Wikisource-TR as "CC0/PD" and their transcriptions are CC-BY-SA, which is not on D15's
allow-list for a shipped artefact. The generator refuses them mechanically. `dict_hit_rate`
returns `None` for both — not zero — so nothing downstream misreads the absence as evidence.
**Open for `DECISIONS.md`: which sources build the DE and TR lists.**

## Phase 1 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-10:

1. **Every named test exists and passes** — all twenty rows of the Phase 1 table (1.1–1.20), plus
   about twenty more, each of which exists because something was measured and was not what the
   plan assumed. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
2. **`cargo nextest run --workspace`** — 83 passed, 0 skipped, 0 ignored; with
   `--features poppler-oracle`, 55 passed in `oc-pdf`. Verified on Windows at the time; the
   three-OS claim was cashed on 2026-09-13, when CI first ran (see Notes), and it needed two
   fixes in this phase's code to hold — the tagged-fixture dependency and h01's host-dependent
   glyph boxes.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources ok; `deny.tools.toml` ok.
6. **`cargo run -p xtask -- thresholds-lint`** — clean.
7. **Acceptance criteria A1.1–A1.6.** A1.1 by `acceptance::every_glyph_carries_thirteen_real_
   signals` over all eleven fixtures; A1.2 by test 1.5; A1.3 by tests 1.10 and VD-d.7; A1.4 by
   test 1.8; A1.5 by 1.16. **A1.6 measured** on a 301-page Typst book: 10.2 ms/page against a
   150 ms budget, 98 MB peak RSS against 250 MB — but **not on D9's reference machine L**, so it
   is indicative rather than signed off. Phase 7's benchmark harness measures it properly.
8. **`docs/CHANGELOG.md`** — Phase 1 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

**VD-d closed**: PDFium's `get_processed_image()` composites soft masks, stencil masks, indexed
palettes and DeviceGray correctly, so the image policy uses it and writes no compositing of its
own. CMYK JPEG, 1-bit CCITT and JPX are uncovered — no encoder exists to build those fixtures
honestly — and are deferred to real samples in the Phase 7 corpus.

## Phase 0 — Definition of Done

Checked against `IMPLEMENTATION_PLAN.md` §0.3 on 2026-09-09:

1. **Every named test exists and passes** — all 23 rows of the Phase 0 table, plus four additions
   (0.8a, 0.12a, 0.23a, and the committed-assertion-file test), each with its reason in
   `docs/DECISIONS_LOG.md`. `docs/TEST_MATRIX.md` lists every one and the CI job that runs it.
2. **`cargo nextest run --workspace`** — green, 0 skipped, 0 ignored. Verified on Windows at the
   time, and the entry said plainly that Linux and macOS "have not run yet". They ran for the
   first time on 2026-09-13 and are green (see Notes). The caveat was correct and it stood for
   four days longer than anyone noticed, because `ci` triggers on `main` and this phase was
   never on it.
3. **clippy** `--workspace --all-targets --all-features --locked -- -D warnings` — clean.
4. **`cargo fmt --all --check`** — clean.
5. **`cargo deny check`** — advisories, bans, licenses, sources all ok; plus `deny.tools.toml`
   licenses/bans/sources ok.
6. **`cargo run -p xtask -- thresholds-lint`** — clean.
7. **Acceptance criteria** A0.1–A0.9: A0.1 (minus the two other OSes, see 2), A0.2, A0.3, A0.4, A0.5,
   A0.7, A0.8 and A0.9 are demonstrated by named tests. **A0.6 is partly open**: the app compiles, the
   handshake and the rendered version string are unit-tested, but no one has watched a real window open.
   Phase 12 owns the UI; a screenshot at that point closes it.
8. **`docs/CHANGELOG.md`** — Phase 0 entry written.
9. **No unnumbered TODO/FIXME** — `xtask ci-lint` clean.

## Completed items log

<!-- one line per finished work item: `YYYY-MM-DD  P<phase>.<item>  <what>  <commit sha>` -->
2026-09-09  P0.setup  Cargo workspace, thresholds.toml, deny.toml, CI matrix, eval skeleton (§1.1-§1.9)  e6d02df
2026-09-09  P0.1      oc-model: BlockId derivation + Rect (tests 0.1, 0.2)                          1d32410
2026-09-09  P0.2      oc-model: canonical JSON + IR_VERSION (tests 0.3, 0.4)                        2cd8f76
2026-09-09  P0.3      oc-core: thresholds codegen + provenance lint (tests 0.5, 0.6)                3843abb
2026-09-09  P0.4      oc-pdf: page-space normalisation (test 0.8 + corner unit test)              79ac71f
2026-09-09  P0.5      xtask vendor-pdfium + oc-pdf PDFium binding and probe (test 0.7)             5059a98
2026-09-09  P0.6      oc-pdf: page classification (tests 0.9-0.12 + mixed/blank/dict test)        97ddfd8
2026-09-09  P0.7      oc-pdf: producer-family detection (test 0.13)                              4788213
2026-09-09  P0.8      xtask fixtures + audit-surface split resolving Q1 (test 0.20)              57df06e
2026-09-09  P0.9      oc-pdf: inspect report + PdfDoc over PDFium (tests 0.14-0.16)             3ec6f70
2026-09-09  P0.10     openconvert: CLI, NDJSON events, exit codes (tests 0.17-0.19)             931cc0a
2026-09-09  P0.11/12  oc-testkit assertions + xtask ci-lint/thresholds-lint (0.21, 0.23)        de4298c
2026-09-09  P0.13     apps/desktop hello-Tauri + handshake + LICENSE (test 0.22)                52a4071
2026-09-09  PHASE 0   COMPLETE - Definition of Done checked, one item partly open (A0.6)
2026-09-09  P1.1      oc-testkit handmade fixtures + the PDFium overdraw finding           15d0dbe
2026-09-09  P1.2      oc-model extract/ledger + oc-pdf glyph extraction (tests 1.1-1.4)    f962f00
2026-09-09  P1.3      oc-pdf metamorphic invariants + oc-testkit mutate (tests 1.5-1.7)   bd96e3a
2026-09-09  P1.4      oc-pdf broken-text: control-char counter + strip_tounicode (test 1.8)  928f9d7
2026-09-09  P1.5      oc-pdf images: ImageRef, DPI, kind, smask/inline via lopdf (test 1.9)  053ad54
2026-09-09  P1.6      oc-core/oc-pdf resource limits + --max-pages (tests 1.10, 1.11, 1.20)  13fce0b
2026-09-09  P1.7      oc-pdf encryption: permissions recorded not enforced (tests 1.12-1.14)  a825fc5
2026-09-10  P1.8      oc-pdf outline walk + meta from the object tree (test 1.15)  d815ad3
2026-09-10  P1.9      oc-pdf fuzz-lite: random, truncated and corrupted inputs (test 1.16)  d9f18aa
2026-09-10  P1.10     oc-pdf dump + openconvert dump-stage ingest (test 1.17)  0ccbd1b
2026-09-10  P1.11     oc-pdf differential pdftotext oracle behind a feature (test 1.18)  2a9aa8f
2026-09-10  P1.perf   oc-pdf: page ids read once, not per page (O(n^2) fix)                6908601
2026-09-10  P1.12     oc-core cancel/progress + openconvert control channel (test 1.19)  69734e5
2026-09-10  P1.13     oc-pdf image_bytes + VD-d known-answer spike (VD-d closed)             c73476e
2026-09-10  PHASE 1   COMPLETE - Definition of Done checked; A1.6 measured off reference machine L
2026-09-13  P2.1      oc-core ledger_check: I-1..I-4 + stage declarations (2.16, 2.17 + 6)  fad617e
2026-09-13  P2.2      oc-text normalize: N, ledgered on both sides (2.1-2.4 + 3)             b7a822c
2026-09-13  P2.3      oc-text fold_key + oc-model LangTag (2.6, 2.7 + 4)                     b5e08ad
2026-09-13  P2.4      oc-pdf: decode the U+0002 hyphen marker at extraction (3 tests)        e5ada15
2026-09-13  P2.5      oc-text words/lines + oc-model text layer, h16-h18 (2.5, 2.8, 2.9 + 9) c49ef42
2026-09-13  P2.6      oc-layout furniture + h19-h21 (2.10-2.14 + 3)                          787002f
2026-09-13  P2.7      openconvert lib: text+furniture under check_invariants (2.15 + 4)      9ce1585
2026-09-13  P2.8      oc-text stats: the nine Gopher numbers + verdict (2.18 + 5)            b20c293
2026-09-13  P2.doc    example_pdfs recorded in TEST_CORPUS 7.5a as the local smoke set       b7f2037
2026-09-13  P2.9      oc-text freq + wordfreq.py; EN ships, DE/TR blocked on D15 (2.22 + 7)  bb11165
2026-09-13  P2.10     oc-text lang + f04/f05 fixtures (2.19, 2.20 + 7)                       a184c12
2026-09-13  P2.11     openconvert dump-stage text + snapshot; min_space_ratio (2.21)         b3949e9
2026-09-13  PHASE 2   COMPLETE - Definition of Done checked; DE/TR frequency lists open (D15)
2026-09-13  P3.1      VD-b closed: hyphenation banned, not depended on                            059c00d
2026-09-13  P3.2      oc-layout blocks: Docstrum + whitespace cover + layout stage (3.1, 3.13 + 6) 36874a1
2026-09-13  P3.3      oc-layout columns + reading order; f02 rewritten (3.2-3.4 + 7)               adb50d9
2026-09-13  P3.4      oc-layout continuity + h22; the retry compares hypotheses (3.5 + 7)          7fa85aa
2026-09-13  P3.5      oc-layout paragraphs: convention, unwrap factor, cross-page merge (3.6 + 8)  0ad7b6a
2026-09-13  P3.6      oc-text dehyphen tiers + paragraphs stage + I-5 + h23 (3.7/3.8/3.10/3.11/3.17) 7979668
2026-09-13  P3.7      oc-text compound_de + f06; the capital after the hyphen (3.9 + 8)            f417036
2026-09-13  P3.8      oc-text classifier + hyphen_clf.py + holdout; keep-recall 0.912 (3.12 + 6)   650a3e6
2026-09-13  P3.9      openconvert dump_layout + anchor + drop caps; the cover fixed (3.14-3.16 + 12) 0dfd45c
2026-09-13  PHASE 3   COMPLETE - Definition of Done checked; A3.2 and A3.3 partial, both named
2026-09-14  P4.1      oc-model doc: the semantic layer + 5 id types; Para grows its half (3)     ac2218b
2026-09-14  P4.2      oc-pdf VectorRegion + is_rule + h24; the rule predicate (3)                316611f
2026-09-14  P4.3      oc-structure cluster + validity gate; text gets a font table (4.3/4.6 + 4) 5b047af
2026-09-14  P4.4      oc-pdf: an outline entry is read once; f07-f10 land (1)                    697099b
2026-09-14  P4.5      oc-structure headings: outline/TOC/size-rank + the style barrier (4.1-4.5) 6340775
2026-09-14  P4.6      oc-structure notes + the raised-ink superscript rule (4.7, 4.8)            d00443d
2026-09-14  P4.7      oc-structure figures: captions, and the abstention (4.9, 4.10)             a434a4c
2026-09-14  P4.8      oc-structure lists + tables, and the honest fallback (4.11-4.14 + 5)       9b2b2ad
2026-09-14  P4.9      oc-structure quotes + meta; XMP over boilerplate (4.16-4.18 + 4)           4076f40
2026-09-14  P4.10     openconvert structure stage under the conservation law (4.15, 4.19-4.21)   70439b6
2026-09-14  P5.1      oc-model Document + the document stage; page breaks, class, preset (10)   5d3717a
2026-09-14  P5.2      oc-epub: the typed XHTML builder; two compile-fail rows (5.1, 5.2 + 14)    b100485
2026-09-14  P5.3      oc-epub: the deterministic OCF container (5.3 + 3)                          b57bfad
2026-09-14  P5.4      oc-epub: one stylesheet that names no typeface (5.12 + 2)                   c7cd1e5
2026-09-14  P5.5      oc-structure: spans carry their styles and their note references (2)        ea69e20
2026-09-14  P5.6      oc-epub: content documents, package, nav, ncx, images, container (5.4-5.14, 5.20 + 14)  24087ae
2026-09-14  P5.7      oc-validate: Tier 1, against real and crafted output (5.15, 5.16 + 6)       a4b8e7b
2026-09-14  P5.8      openconvert: convert + validate; the pipeline moved into the library (5.19 + 5)  19c31b1
2026-09-18  P6.1      oc-validate: I-7 over the archive, and retention as a flag (6.1-6.3 + 4)     c5a5627
2026-09-18  P6.2      oc-validate: the structural report - headings, duplicates, quality (6.16 + 15)  3bca6fd
2026-09-18  P6.3      oc-validate: the repair loop, its measure and its table (6.4-6.7, 6.9 + 12)  7b35583
2026-09-18  P6.4      openconvert: the loop on the real path, fire rate zero (6.10 + 5)              9889827
2026-09-18  P6.5      openconvert: report.json, --report, the post-cap policy (6.8, 6.12 + 4)        e445783
2026-09-18  P6.6      oc-core: the warning registry, en/de/tr templates, the registry lint (6.11 + 7)   167227d
2026-09-18  P6.7      tests/dom: the Playwright DOM checks, three viewports, CI on (6.13-6.15 + 4)    4a71fc0
2026-09-18  P6.8      oc-validate: the Tier-3 Ace runner and the nightly ace-a11y job (5 tests)      5855c53
2026-09-19  P6.ci     seven defects CI found: cross-OS bytes, Ace a11y x3, disk, tar/zip     23c7064
2026-09-20  P7.1      corpus: sha256 before use, mirror-then-source, the LOCAL_EVAL boundary (9)   3bf9c1c
2026-09-20  P7.2      corpus: the manifest vocabulary and fourteen lint rules (7.1, 7.3b + 21)     5281b83
2026-09-20  P7.3      corpus: the frozen holdout, 104 real documents, five sources (7.2, 7.3 + 50)  86848f8
2026-09-20  P7.4      oc-testkit: the mutation catalogue, ten recipes with declared effects (7.6 + 3)  3b55dbb
2026-09-20  P7.5      oc-eval: ground truth from XHTML, struct trees and LaTeX (7.7 + 16)     36b6384
2026-09-20  P7.6      oc-eval: metrics, the Wilson gate, the per-stratum report (7.8, 7.9 + 21)  ee146db
2026-09-20  P7.7      oc-eval: the ours-vs-real gap recorded and plotted (7.10 + 7)      8a056c0
2026-09-20  P7.8      oc-eval: calibration refuses the holdout; risk-coverage (7.4 + 11)  bb253f3
2026-09-20  P7.9      openconvert: the perf budget, 0.0217 s/page on 300 pages (7.11, 7.12 + 14)  1f94be4
2026-09-20  P7.10     ci: the python job, corpus lint, and four nightly bodies (7.13, 7.14 + 16)  61edb1d
