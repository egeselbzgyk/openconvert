// The four indented categories PIPELINE §8.6 calls geometrically indistinguishable:
// a block quotation, a stanza of verse, an epigraph and a preformatted block. Each is
// indented, short-lined and unjustified, and no public layout dataset has any of the
// classes (DocLayNet's eleven contain none of them, R10 §6.13). The fixture exists so that
// the deterministic default — blockquote if indented, paragraph if not — can be checked to
// resolve the ambiguous case *and* to record it as an escalation candidate rather than to
// guess quietly.
#set document(title: "Verse and Quotation", author: ("O. Convert",), date: none)
#set page(
  width: 148mm, height: 210mm, margin: (x: 18mm, y: 18mm),
  header: context [#set text(8pt); #align(center)[Verse and Quotation]],
  footer: context [#set text(8pt); #align(center)[#counter(page).display()]],
)
#set text(font: "Libertinus Serif", size: 10pt, lang: "en")
#set par(justify: true, leading: 0.65em, first-line-indent: 1.2em)

= Chapter One

The body text of this fixture is ordinary justified prose set flush to the left
margin, so that every indented block below it has an unindented baseline to be
measured against. Without that baseline the indent delta has no zero and the
z-score is meaningless.

#block(inset: (left: 2em))[
  It is a truth universally acknowledged, that a single man in possession of a
  good fortune, must be in want of a wife. However little known the feelings or
  views of such a man may be on his first entering a neighbourhood, this truth is
  so well fixed in the minds of the surrounding families.
]

The paragraph above is a block quotation: indented on the left, set as continuous
prose, and long enough that its lines fill the measure. Its short-line ratio is
low, which is the signal that separates it from the stanza below.

#block(inset: (left: 2em))[
  #set par(justify: false, first-line-indent: 0em)
  Tyger Tyger, burning bright, \
  In the forests of the night; \
  What immortal hand or eye, \
  Could frame thy fearful symmetry? \
]

The stanza above is verse: indented like the quotation, but every line ends short
of the measure, so its short-line ratio is high. Geometry alone cannot tell the
two apart when the ratio falls between them, and that is the case this fixture is
built to leave ambiguous.

#block(inset: (left: 2em))[
  #set par(justify: false, first-line-indent: 0em)
  A middle case, indented, whose lines \
  are neither full nor consistently short, \
  so that the short-line ratio lands between \
  the two bounds and nothing decides it.
]

The block above is the ambiguous one. It is indented and its short-line ratio sits
in the interval where PIPELINE §8.6 says the deterministic answer is blockquote and
the decision is recorded as an escalation candidate with its signals.
