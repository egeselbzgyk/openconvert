// Footnote linkage. R2 §B.6: getting the *linkage* right matters more than getting the
// classification right, and the bijection noteref <-> footnote is what prevents the
// EPUBCheck RSC-007/RSC-012 broken-reference class (R10 §6.9, R5 §B6). Calibre has no
// footnote handling at all (R1 §C.2 #4), so this is a fixture about doing something
// nothing else in the field does rather than about matching anyone.
#set document(title: "Notes at the Foot", author: ("O. Convert",), date: none)
#set page(
  width: 148mm, height: 210mm, margin: (x: 18mm, y: 18mm),
  header: context [#set text(8pt); #align(center)[Notes at the Foot]],
  footer: context [#set text(8pt); #align(center)[#counter(page).display()]],
)
#set text(font: "Libertinus Serif", size: 10pt, lang: "en")
#set par(justify: true, leading: 0.65em, first-line-indent: 1.2em)
#show footnote.entry: set text(8pt)

= Chapter One

The first paragraph of the chapter carries a reference#footnote[The first note,
set at eight point beneath a short rule at the foot of its own page.] in the
middle of a sentence, so that the marker is a superscript inside a body line
rather than a line of its own.

A second paragraph follows with a reference of its own#footnote[The second note.
Two notes on one page are what makes ordering an independent signal from symbol
equality.] near its end, which puts two notes in the zone at the foot of the same
page and forces the matcher to order them rather than to guess.

The third paragraph is long enough to push the page on, and carries no reference
at all, because a paragraph without a marker has to survive a matcher that walks
markers and bodies in parallel.

#pagebreak()

= Chapter Two

The second chapter opens with a reference#footnote[The third note, on the second
page. Numbering continues across pages, so a matcher that resets its counter per
page links this to the first note instead.] in its first sentence.

The final paragraph closes the fixture without a marker, so that the document
ends on unreferenced text.
