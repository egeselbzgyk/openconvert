// f04 — one page of German prose, for `dc:language` detection (test 2.19).
//
// The text is written for this fixture rather than quoted, so there is no third-party
// licence attached to a file the corpus redistributes (D18, TEST_CORPUS §1.1). What it has
// to be is *ordinary* German — the umlauts, the ß, the compound nouns and the verb-final
// subordinate clauses that `whatlang`'s trigram model keys on — and not a sentence
// constructed to be easy to detect.
#set document(title: "Der Herbst im Dorf", author: ("O. Convert",), date: none)
#set page(width: 148mm, height: 210mm, margin: (x: 18mm, y: 18mm))
#set text(font: "Libertinus Serif", size: 10pt, lang: "de")
#set par(justify: true, leading: 0.65em, first-line-indent: 1.2em)

#heading(level: 1)[Der Herbst im Dorf]

Der Herbst kam früh in diesem Jahr. Über den Feldern lag ein grauer Nebel, und die Bäume
an der Straße hatten ihre Blätter schon verloren. Der alte Müller saß vor seinem Haus und
rauchte eine Pfeife, während die Kinder aus dem Dorf zwischen den Scheunen spielten.

Am Abend wurde es kalt, und niemand blieb länger draußen als nötig. Aus den Fenstern der
Häuser fiel gelbes Licht auf den nassen Weg, und im Gasthaus an der Kreuzung sprachen die
Männer über die Ernte, über das Wetter und über den Winter, der bald kommen würde.

Später hörte man nur noch den Wind in den Kastanien und irgendwo einen Hund, der gegen die
Dunkelheit bellte. Die Straße lag leer, und der Nebel stieg langsam vom Fluss herauf.
