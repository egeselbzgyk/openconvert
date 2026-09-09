#set document(title: "The Test Book", author: ("O. Convert",), date: none)
#set page(
  width: 148mm, height: 210mm, margin: (x: 18mm, y: 18mm),
  header: context [#set text(8pt); #align(center)[The Test Book]],
  footer: context [#set text(8pt); #align(center)[#counter(page).display()]],
)
#set text(font: "Libertinus Serif", size: 10pt, lang: "en")
#set par(justify: true, leading: 0.65em, first-line-indent: 1.2em)

#heading(level: 1)[Chapter 3]

It was a dark and stormy night; the rain fell in torrents, except at occasional
intervals, when it was checked by a violent gust of wind which swept up the
streets, rattling along the housetops, and fiercely agitating the scanty flame
of the lamps that struggled against the darkness.

The office was quiet. A single clerk remained at his desk, copying a schedule of
freight rates in a hand so regular that the page might have been printed. He did
not look up when the door opened, nor when it closed again.

Outside, the harbour lights went out one by one, and the last of the coasting
steamers slipped her moorings and stood away for the open sea.

#pagebreak()

A second page follows, so that running-header and page-number detection has more
than one page of evidence to work with, and so that cross-page paragraph
continuation can be exercised by later phases of the pipe-
line without another fixture.
