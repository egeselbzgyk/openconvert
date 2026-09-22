# String inventory (EN keys, DE/TR drafts)

Drafts for layout testing only. Final strings need native review. `{name}` placeholders are filled by the formatter. Numbers use `Intl.NumberFormat` per locale (1,842 / 1.842 / 1.842; 62 % / 62 % / %62). Warning strings are keyed by engine `WarningCode`, and CI fails if a code lacks a template in any locale.

Turkish casing: never use `text-transform`. Casing comes from the string itself (İ/i, I/ı). Set `lang="tr"` on the root so hyphenation and any `toLocaleUpperCase` calls use Turkish rules.

| Key | EN | DE | TR |
|---|---|---|---|
| `app.title` | OpenConvert | OpenConvert | OpenConvert |
| `app.settings` | Settings | Einstellungen | Ayarlar |
| `drop.title` | Drop PDF here | PDF hier ablegen | PDF'yi buraya bırakın |
| `drop.or` | or | oder | veya |
| `drop.select` | Select PDF… | PDF auswählen… | PDF seçin… |
| `drop.more` | Drop more PDFs here | Weitere PDFs hier ablegen | Daha fazla PDF'yi buraya bırakın |
| `drop.release` | Release to add {n} PDFs | Loslassen, um {n} PDFs hinzuzufügen | {n} PDF eklemek için bırakın |
| `drop.skipped` | {file} is not a PDF and will be skipped. | {file} ist keine PDF und wird übersprungen. | {file} bir PDF değil, atlanacak. |
| `privacy.line` | Everything runs on this computer: no account, no telemetry, no network use while converting. | Alles läuft auf diesem Computer: kein Konto, keine Telemetrie, keine Netzwerknutzung beim Konvertieren. | Her şey bu bilgisayarda çalışır: hesap yok, telemetri yok, dönüştürme sırasında ağ kullanımı yok. |
| `firstrun.body` | AI-assisted structure detection is available and off by default. It can improve heading and chapter detection on ambiguous documents. | KI-gestützte Strukturerkennung ist verfügbar und standardmäßig aus. Sie kann Überschriften und Kapitel in uneindeutigen Dokumenten besser erkennen. | Yapay zekâ destekli yapı algılama kullanılabilir ve varsayılan olarak kapalıdır. Belirsiz belgelerde başlık ve bölüm algılamayı iyileştirebilir. |
| `firstrun.costs` | Costs: | Kosten: | Maliyet: |
| `firstrun.cost.download` | ~1,100 MB download | ~1.100 MB Download | ~1.100 MB indirme |
| `firstrun.cost.ram` | ~1–2 GB RAM while converting | ~1–2 GB RAM beim Konvertieren | dönüştürürken ~1–2 GB RAM |
| `firstrun.cost.time` | a few extra seconds per book | ein paar Sekunden mehr pro Buch | kitap başına birkaç saniye fazla |
| `firstrun.setup` | Set up AI assistance | KI-Unterstützung einrichten | Yapay zekâ desteğini kur |
| `firstrun.notnow` | Not now | Nicht jetzt | Şimdi değil |
| `queue.title` | Queue | Warteschlange | Kuyruk |
| `queue.count` | {n} files · {r} converting · {w} waiting | {n} Dateien · {r} wird konvertiert · {w} warten | {n} dosya · {r} dönüştürülüyor · {w} bekliyor |
| `queue.queued` | Queued (#{pos}) | In Warteschlange (Nr. {pos}) | Sırada ({pos}.) |
| `queue.startsAfter` | starts after the file above | startet nach der Datei darüber | üstteki dosyadan sonra başlar |
| `queue.remove` | Remove | Entfernen | Kaldır |
| `queue.cancel` | Cancel | Abbrechen | İptal |
| `queue.cancelling` | Cancelling… | Wird abgebrochen… | İptal ediliyor… |
| `queue.cancelled` | Cancelled | Abgebrochen | İptal edildi |
| `queue.noFile` | no file was written | es wurde keine Datei geschrieben | hiçbir dosya yazılmadı |
| `queue.again` | Convert again | Erneut konvertieren | Yeniden dönüştür |
| `queue.notResponding` | Not responding | Reagiert nicht | Yanıt vermiyor |
| `queue.notRespondingDetail` | no signal from the converter for {s} seconds. It may recover on its own. | seit {s} Sekunden kein Signal vom Konverter. Er kann sich von selbst erholen. | dönüştürücüden {s} saniyedir sinyal yok. Kendiliğinden düzelebilir. |
| `queue.stillWorking` | still working | arbeitet noch | hâlâ çalışıyor |
| `queue.stepOf` | step {i} of {n} | Schritt {i} von {n} | {n} adımdan {i}. |
| `stage.analyzing` | Analyzing | Wird analysiert | Analiz ediliyor |
| `stage.extracting` | Extracting | Wird extrahiert | Ayıklanıyor |
| `stage.reconstructing` | Reconstructing | Wird rekonstruiert | Yeniden yapılandırılıyor |
| `stage.building` | Building | Wird erstellt | Oluşturuluyor |
| `stage.checking` | Checking | Wird geprüft | Denetleniyor |
| `stage.repairing` | Repairing | Wird repariert | Onarılıyor |
| `stage.complete` | Complete | Fertig | Tamamlandı |
| `progress.pages` | {done} of {total} pages · {pct} | {done} von {total} Seiten · {pct} | {total} sayfanın {done}'i · {pct} |
| `result.open` | Open in reader | Im Reader öffnen | Okuyucuda aç |
| `result.show` | Show in folder | Im Ordner anzeigen | Klasörde göster |
| `result.collapse` | Collapse result | Ergebnis einklappen | Sonucu daralt |
| `result.summary` | {pages} pages · {blocks} blocks · {images} images · {tables} tables · {chapters} chapters | {pages} Seiten · {blocks} Blöcke · {images} Bilder · {tables} Tabellen · {chapters} Kapitel | {pages} sayfa · {blocks} blok · {images} görsel · {tables} tablo · {chapters} bölüm |
| `result.deterministic` | Deterministic processing | Deterministische Verarbeitung | Deterministik işleme |
| `result.aiDecisions` | AI-assisted decisions: {n} | KI-gestützte Entscheidungen: {n} | Yapay zekâ destekli kararlar: {n} |
| `validation.passed` | Validation: passed | Validierung: bestanden | Doğrulama: geçti |
| `validation.warn` | Validation: passed with warnings ({n}) | Validierung: bestanden, mit Warnungen ({n}) | Doğrulama: uyarılarla geçti ({n}) |
| `validation.invalid` | Validation: invalid | Validierung: ungültig | Doğrulama: geçersiz |
| `validation.invalidNote` | The EPUB was saved and can still be opened. Some readers may refuse it. | Das EPUB wurde gespeichert und lässt sich weiterhin öffnen. Manche Reader lehnen es eventuell ab. | EPUB kaydedildi ve yine de açılabilir. Bazı okuyucular reddedebilir. |
| `page.short` | p. {n} | S. {n} | s. {n} |
| `warn.W_DEHYPHEN_LOWCONF` | Low-confidence hyphenation join kept as hyphenated | Unsichere Worttrennung: Bindestrich beibehalten | Emin olunamayan heceleme birleştirmesi tireli bırakıldı |
| `warn.W_IMAGE_LOWRES` | {n} image below minimum resolution | {n} Bild unter der Mindestauflösung | {n} görsel en düşük çözünürlüğün altında |
| `warn.W_TABLE_AS_IMAGE` | {n} tables were kept as images because their structure could not be recovered reliably. | {n} Tabellen wurden als Bilder beibehalten, weil ihre Struktur nicht zuverlässig erkannt wurde. | {n} tablo, yapıları güvenilir biçimde çıkarılamadığı için görsel olarak bırakıldı. |
| `warn.W_IMAGE_ONLY_PAGES` | Pages {from}–{to} are scanned images. OCR isn't installed, so they were kept as images. | Die Seiten {from}–{to} sind gescannte Bilder. OCR ist nicht installiert, daher wurden sie als Bilder beibehalten. | {from}–{to}. sayfalar taranmış görsel. OCR yüklü olmadığı için görsel olarak bırakıldı. |
| `warn.W_HEADING_LEVEL_SKIP` | Heading levels jump from {a} to {b} on p. {p}. | Die Überschriftenebenen springen auf S. {p} von {a} auf {b}. | {p}. sayfada başlık düzeyi {a}'den {b}'e atlıyor. |
| `warn.W_NOTE_UNMATCHED` | {n} footnote on p. {p} has no matching reference in the text. | {n} Fußnote auf S. {p} hat keinen passenden Verweis im Text. | {p}. sayfadaki {n} dipnotun metinde eşleşen bir göndermesi yok. |
| `warn.W_EPUB_LARGE` | The EPUB is larger than 50 MB; some readers may open it slowly. | Das EPUB ist größer als 50 MB; manche Reader öffnen es langsam. | EPUB 50 MB'tan büyük; bazı okuyucular yavaş açabilir. |
| `warn.W_VALIDATION_UNREPAIRABLE` | The EPUB did not pass validation after {n} repair attempts. It was saved and marked invalid. | Das EPUB hat die Validierung nach {n} Reparaturversuchen nicht bestanden. Es wurde gespeichert und als ungültig markiert. | EPUB, {n} onarım denemesinden sonra doğrulamayı geçemedi. Kaydedildi ve geçersiz olarak işaretlendi. |
| `facts.retention` | Character retention | Zeichenerhalt | Karakter korunumu |
| `facts.images` | Images | Bilder | Görseller |
| `facts.notes` | Notes linked | Verknüpfte Anmerkungen | Bağlanan notlar |
| `facts.epubcheck` | EPUBCheck | EPUBCheck | EPUBCheck |
| `facts.notRun` | Not run | Nicht ausgeführt | Çalıştırılmadı |
| `facts.packMissing` | validation pack not installed | Validierungspaket nicht installiert | doğrulama paketi yüklü değil |
| `facts.of` | {a} of {b} | {a} von {b} | {a}/{b} |
| `action.details` | Details | Details | Ayrıntılar |
| `action.preview` | Preview | Vorschau | Önizleme |
| `action.editMeta` | Edit metadata | Metadaten bearbeiten | Üst verileri düzenle |
| `action.reviewToc` | Review TOC | Inhaltsverzeichnis prüfen | İçindekileri gözden geçir |
| `action.exportDiag` | Export diagnostic bundle | Diagnosepaket exportieren | Tanılama paketini dışa aktar |
| `action.fixRebuild` | Fix and rebuild | Korrigieren und neu erstellen | Düzelt ve yeniden oluştur |
| `preview.label` | Approximate preview — your reading device may differ. | Ungefähre Vorschau – Ihr Lesegerät kann abweichen. | Yaklaşık önizleme — okuma cihazınız farklı görünebilir. |
| `error.notPdf` | This file doesn't look like a valid PDF. | Diese Datei scheint keine gültige PDF zu sein. | Bu dosya geçerli bir PDF gibi görünmüyor. |
| `error.damaged` | This PDF is too damaged to read. | Diese PDF ist zu beschädigt, um sie zu lesen. | Bu PDF okunamayacak kadar hasarlı. |
| `error.sameResult` | Converting it again would give the same result. | Eine erneute Konvertierung würde dasselbe Ergebnis liefern. | Yeniden dönüştürmek aynı sonucu verir. |
| `error.password` | This PDF is password-protected. | Diese PDF ist passwortgeschützt. | Bu PDF parola korumalı. |
| `error.passwordOnce` | Used for this job only, never saved. | Nur für diesen Auftrag verwendet, nie gespeichert. | Yalnızca bu iş için kullanılır, asla kaydedilmez. |
| `error.unlock` | Unlock | Entsperren | Kilidi aç |
| `error.memory` | This document exceeds the memory limit for a single conversion. | Dieses Dokument überschreitet das Speicherlimit für eine einzelne Konvertierung. | Bu belge tek bir dönüştürme için bellek sınırını aşıyor. |
| `error.memoryHint` | You can raise "Max memory" in Settings › Advanced and convert it again. | Sie können „Max. Speicher" unter Einstellungen › Erweitert erhöhen und erneut konvertieren. | Ayarlar › Gelişmiş bölümünden "En fazla bellek" değerini artırıp yeniden dönüştürebilirsiniz. |
| `banner.aiUnavailable` | AI assistance unavailable this run — converted deterministically. | KI-Unterstützung in diesem Durchlauf nicht verfügbar – deterministisch konvertiert. | Yapay zekâ desteği bu çalıştırmada kullanılamadı — deterministik olarak dönüştürüldü. |
| `banner.fallback` | {a} could not be loaded on this computer. {b} was used instead. | {a} konnte auf diesem Computer nicht geladen werden. Stattdessen wurde {b} verwendet. | {a} bu bilgisayarda yüklenemedi. Yerine {b} kullanıldı. |
| `command.copy` | Copy | Kopieren | Kopyala |
| `command.copied` | Copied | Kopiert | Kopyalandı |
| `consent.title` | Send document text to {host}? | Dokumenttext an {host} senden? | Belge metni {host} adresine gönderilsin mi? |
| `consent.body` | This endpoint is not on this computer. When AI assistance runs, text from the documents you convert will leave this computer and be sent to {host}. | Dieser Endpunkt ist nicht auf diesem Computer. Wenn die KI-Unterstützung läuft, verlässt Text aus den konvertierten Dokumenten diesen Computer und wird an {host} gesendet. | Bu uç nokta bu bilgisayarda değil. Yapay zekâ desteği çalıştığında, dönüştürdüğünüz belgelerdeki metin bu bilgisayardan çıkar ve {host} adresine gönderilir. |
| `consent.allow` | Allow {host} | {host} erlauben | {host} için izin ver |
| `startup.protocol` | OpenConvert can't start its converter | OpenConvert kann seinen Konverter nicht starten | OpenConvert dönüştürücüsünü başlatamıyor |
| `startup.version` | The bundled converter doesn't match this app | Der mitgelieferte Konverter passt nicht zu dieser App | Paketlenmiş dönüştürücü bu uygulamayla eşleşmiyor |
| `settings.language` | App language | App-Sprache | Uygulama dili |
