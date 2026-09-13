// f05 — one page of Turkish prose, for `dc:language` detection (test 2.19).
//
// Written for this fixture rather than quoted, so nothing third-party is redistributed
// (D18, TEST_CORPUS §1.1). Turkish is here for three reasons at once and this page carries
// all of them: `whatlang` has to name it, the dotted and dotless i have to survive
// extraction as themselves, and `fold_key` has to pair them the Turkish way rather than the
// Latin way (R10 §6.3). The vowel harmony and the agglutinated suffixes are what the
// trigram model actually keys on, so the text is ordinary prose and not a word list.
#set document(title: "Köyde Sonbahar", author: ("O. Convert",), date: none)
#set page(width: 148mm, height: 210mm, margin: (x: 18mm, y: 18mm))
#set text(font: "Libertinus Serif", size: 10pt, lang: "tr")
#set par(justify: true, leading: 0.65em, first-line-indent: 1.2em)

#heading(level: 1)[Köyde Sonbahar]

Sonbahar bu yıl erken geldi. Tarlaların üzerinde gri bir sis vardı ve yol kenarındaki
ağaçlar yapraklarını çoktan dökmüştü. Yaşlı değirmenci evinin önünde oturmuş pipo
içiyordu; köyün çocukları ise ambarların arasında oynuyorlardı.

Akşam olunca hava soğudu ve kimse gerekenden fazla dışarıda kalmadı. Evlerin
pencerelerinden yola sarı bir ışık düşüyordu, kavşaktaki kahvede ise adamlar hasattan,
havadan ve yaklaşan kıştan söz ediyorlardı.

Daha sonra yalnızca kestane ağaçlarındaki rüzgâr duyuluyordu, bir de karanlığa havlayan
uzak bir köpek. Yol bomboştu ve sis ırmaktan yavaşça yükseliyordu. İstanbul'dan gelen
son araba çoktan geçmişti.
