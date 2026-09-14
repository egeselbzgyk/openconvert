// Lists, a ruled table and a captioned figure — the three structures whose *geometry* is
// crisp enough that heuristics should do well on them. List-item is the best-detected class
// in DocLayNet (86.2 mAP against human 87-88, R10 §6.11); Camelot's lattice parser reaches
// F1 0.778 on ruled tables and materially worse on borderless ones (R2 §B.9); caption
// association is ambiguous even for humans (DocLayNet Caption agreement 84-89, R10 §6.10).
// The fixture is built so each of the three is unambiguous, because the ambiguous cases are
// h25, h26 and f07 and a fixture should test one thing.
#set document(title: "Lists and Tables", author: ("O. Convert",), date: none)
#set page(
  width: 148mm, height: 210mm, margin: (x: 18mm, y: 18mm),
  header: context [#set text(8pt); #align(center)[Lists and Tables]],
  footer: context [#set text(8pt); #align(center)[#counter(page).display()]],
)
#set text(font: "Libertinus Serif", size: 10pt, lang: "en")
#set par(justify: true, leading: 0.65em, first-line-indent: 1.2em)

= Chapter 3

The chapter heading above is set at the largest size in the document and is the
only style at that size, so the size rank puts it at level one and the numbering
regex finds its number.

== Enumerated Procedure

The section heading is bold and smaller than the chapter heading, which is what
puts it at level two. The procedure below has five top-level steps and one of
them has two sub-steps, so the nesting depth is two and no deeper.

+ Open the document and read its outline.
+ Cluster the runs by style and find the body mode.
+ Match the outline entries to heading candidates.
  + Bind each destination to the nearest candidate, not to the page.
  + Record which candidates were left unbound.
+ Assign levels by size rank, refined by the numbering regex.
+ Validate: no level skips, and the order is monotone with the pages.

The list ends and ordinary prose resumes, so that the last item has a following
block which is plainly not an item.

#pagebreak()

== A Ruled Table

#figure(
  table(
    columns: 4,
    stroke: 0.5pt,
    [Stage], [Kind], [Budget], [Reason],
    [text], [Budgeted], [0.005], [SoftHyphen],
    [layout], [Conserving], [0.000], [None],
  ),
  caption: [Stages and their budgets.],
)

Every row of the table above has four cells and every cell holds one short
string, so the cell-text multiset equals the source text multiset exactly. That
equality is the conservation check PIPELINE §8.7 requires, and it is what would
catch a hallucinated cell if a vision model were ever added.

#pagebreak()

== A Captioned Figure

#figure(
  image("../assets/scan_page_01.png", width: 40%),
  caption: [Figure of a scanned page.],
)

The caption sits directly beneath the image and there is no second figure on this
page, so the ratio of second-best to best distance is unbounded and the
association is made. The ambiguous arrangement — two figures equidistant from one
caption — is h25, where the association is refused instead.
