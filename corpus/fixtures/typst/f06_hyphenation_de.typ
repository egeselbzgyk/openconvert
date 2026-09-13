// f06 — German hyphenation: a compound broken at its own hyphen, and a compound broken
// inside a word (test 3.9).
//
// The plan calls this fixture `f04`; `f04` was spent on `f04_german_prose` in Phase 2, so it
// is `f06` here and the mapping is in `docs/TEST_MATRIX.md` (PROGRESS.md sets the rule: the
// test name is the contract, the fixture number is indicative).
//
// The two breaks are forced rather than left to Typst's line breaker, and that is deliberate.
// What the fixture has to guarantee is that `Nord-` ends a line and `Süd-Achse` begins the
// next one, because that is the case under test; a fixture whose subject moves when a
// hyphenation pattern is updated upstream is a fixture that tests the upstream. The geometry
// a forced break produces is the same geometry a natural one does.
//
// The text is written for this fixture rather than quoted, so no third-party licence attaches
// to a file the corpus redistributes (D18, TEST_CORPUS §1.1).
#set document(title: "Die Nord-Süd-Achse", author: ("O. Convert",), date: none)
#set page(width: 148mm, height: 210mm, margin: (x: 18mm, y: 18mm))
#set text(font: "Libertinus Serif", size: 10pt, lang: "de")
#set par(justify: false, leading: 0.65em, first-line-indent: 1.2em)

#heading(level: 1)[Die Nord-Süd-Achse]

Die Stadt plante seit Jahren eine neue Nord-#linebreak()Süd-Achse durch das alte
Viertel. Der Plan lag im Rathaus aus, und jeder konnte ihn dort einsehen.

Die Eisenbahn führte damals noch mitten durch die Stadt. Wer von Norden kam, musste
an der alten Eisen-#linebreak()bahn entlanggehen, bis er den Fluss erreichte.

Gegen den Plan sprach vor allem der Lärm. Die Anwohner schrieben Briefe, hielten
Versammlungen ab und sammelten Unterschriften gegen die Nord-Süd-Achse.
