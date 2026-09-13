#set document(title: "Two Column Study", author: ("O. Convert",), date: none)
#set page(
  width: 210mm, height: 297mm, margin: (x: 20mm, y: 20mm), columns: 2,
  header: context [#set text(8pt); #align(right)[Two Column Study]],
  footer: context [#set text(8pt); #align(center)[#counter(page).display()]],
)
#set text(font: "Libertinus Serif", size: 9.5pt, lang: "en")
#set par(justify: true, leading: 0.6em)

#place(top + center, scope: "parent", float: true)[
  #set text(16pt, weight: "bold")
  #align(center)[On the Measurement of Columns]
]

= Introduction
Column detection is the first stage at which a geometric error becomes visible to
a reader, because a misplaced gutter interleaves two unrelated sentences into one
paragraph. This fixture therefore uses a wide gutter and a floating full-width
title, which is the arrangement that defeats a naive projection profile.

A column is not a property of the page description. Nothing in a PDF says that
these lines belong together and those do not; there are only glyphs, each with a
position, and the columns are an inference drawn from where the glyphs are not.
That is why the whitespace between them carries more information than the text
does, and why a detector that reads the gaps outperforms one that reads the runs.

The gutter of this page is the widest vertical band of emptiness that survives
from the top of the text area to the bottom of it. Every other band fails one of
the three tests: the space between two words is not tall, the space at the end of
a short line is not deep, and the ragged right edge of a paragraph is neither.
A band that passes all three is a gutter, and there is exactly one of them here.

The floating title is the interesting complication. It crosses the gutter near the
top of the page, so a projection taken over the full height of the text area sees
ink in the middle of every column and concludes that the page has one column. The
repair is not a cleverer projection. It is to notice that the title spans what the
rest of the page divides, to set it aside before the cut is made, and to put it
back afterwards at the position a reader meets it: first, above both columns.

= Method
The left column continues for long enough that the reading-order algorithm must
descend the full height of the page before crossing the gutter. Each paragraph is
long enough to wrap several times at this measure, so the projection profile has
real evidence to work with rather than a handful of short lines.

Reading order is then a recursive cut. The page is divided at its widest valley,
each half is divided again at its own widest valley, and the recursion stops when
no valley is wide enough to mean anything. On a Manhattan layout — which is what a
book, a report or a manual is — this is exactly right, and the published numbers
say so: plain recursive cutting scores a hundred per cent on such pages, while the
learned alternative scores ninety-six and collapses on three columns.

#colbreak()

= Results
The right column begins here and continues to the bottom of the page. A correct
reading order emits every line of the left column before the first line of the
right column, on both pages.

A wrong one is not subtly wrong. It reads the first line of the left column, then
the first line of the right, then the second of the left, and produces a paragraph
in which every other sentence belongs to a different argument. That failure is
visible to any reader in the first ten seconds of the book, which is the reason
this fixture exists and the reason the assertion is a binary one.

#pagebreak()

= Discussion
A second page repeats the arrangement so that furniture detection has at least
two samples of the running header and the page number.

= Conclusion
Nothing is concluded; the fixture exists to be measured.
