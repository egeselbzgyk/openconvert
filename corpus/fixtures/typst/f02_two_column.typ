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

= Method
The left column continues for long enough that the reading-order algorithm must
descend the full height of the page before crossing the gutter. Each paragraph is
long enough to wrap several times at this measure.

= Results
The right column begins here and continues to the bottom of the page. A correct
reading order emits every line of the left column before the first line of the
right column, on both pages.

#pagebreak()

= Discussion
A second page repeats the arrangement so that furniture detection has at least
two samples of the running header and the page number.

= Conclusion
Nothing is concluded; the fixture exists to be measured.
