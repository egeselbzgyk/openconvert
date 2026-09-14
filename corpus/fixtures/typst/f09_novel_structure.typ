// A whole book in miniature: front matter with roman folios, a printed table of contents
// with dotted leaders, two numbered chapters with a section inside one of them, and back
// matter. Three independent sources of structure agree on it — the PDF outline, the printed
// TOC page, and the style clusters with the page breaks (IMPLEMENTATION_PLAN Phase 4
// detail 5) — which is what makes it the fixture the outline fast path is checked against
// and the fixture the TOC parser is checked against with the outline taken away.
#set document(title: "A Short Novel", author: ("O. Convert",), date: none)
#set page(
  width: 148mm, height: 210mm, margin: (x: 18mm, y: 18mm),
  header: context [#set text(8pt); #align(center)[A Short Novel]],
)
#set text(font: "Libertinus Serif", size: 10pt, lang: "en")
#set par(justify: true, leading: 0.65em, first-line-indent: 1.2em)

// Roman folios through the front matter, arabic from the first chapter: the arabic-1 reset
// is the hard boundary signal detail 5 names.
#set page(numbering: "i", footer: context [
  #set text(8pt); #align(center)[#counter(page).display("i")]
])

= Preface

This preface stands in the front matter, where the folios are roman. A book that
restarts its numbering at arabic one has told the reader where its body begins,
and that is a stronger signal than any keyword.

#pagebreak()

= Contents

#outline(title: none, depth: 2)

#pagebreak()

#counter(page).update(1)
#set page(numbering: "1", footer: context [
  #set text(8pt); #align(center)[#counter(page).display("1")]
])

= Chapter One

The body of the book begins here, on the first arabic page. The chapter heading is
set larger than the body and bold, it opens its page, and the printed table of
contents two pages earlier names it. All three agree.

The second paragraph runs on for long enough that the chapter is more than a
heading with a sentence under it, because a heading detector that has only one
line of body to compare against has no body mode to speak of.

== A Section Within

The section heading is smaller than the chapter heading and larger than the body,
so the size rank places it at level two. Nothing else distinguishes it: this is
exactly the case where the outline is worth more than the geometry.

The section runs for a paragraph and ends.

#pagebreak()

= Chapter Two

The second chapter opens a new page, as chapters do, which is what lets the
page-break correlation in PIPELINE §8's validation say that the level-one
headings and the page boundaries agree.

A closing paragraph, so that the chapter has a body of its own.

#pagebreak()

= Appendix A

Back matter, named by one of the keywords detail 5 lists in all three target
languages. It follows the body and precedes the index.

= Index

The last section of the book, and the last of the three zones. Front precedes
body precedes back, and the validation says so.
