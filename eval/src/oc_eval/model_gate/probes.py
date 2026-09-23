"""G8's probes: 100 German and 100 Turkish lines, each with one flat-enum role (D9 G8).

The roles are the `heading_roles` grammar's own enum, so a probe asks the model exactly the kind of
question the pipeline asks, in one word. The lines are **generated** from the template pools below
with a fixed seed, not written by a native speaker, and every label follows from the template that
made the line: `Kapitel 3` is a chapter heading because the template says so. A probe set is only as
good as its labels, and a generated set's labels can be reviewed by reading forty lines of pools.

`python -m oc_eval.model_gate.probes --write` regenerates `eval/data/probes/{de,tr}_100.jsonl`;
`test_model_gate.py` holds the committed files equal to what this module generates.
"""

from __future__ import annotations

import argparse
import json
import random
from collections.abc import Callable
from pathlib import Path

from oc_eval import thresholds

REPO_ROOT = Path(__file__).resolve().parents[4]
PROBES_DIR = REPO_ROOT / "eval" / "data" / "probes"
HEADING_ROLES_GRAMMAR = (
    REPO_ROOT / "crates" / "oc-ai" / "prompts" / "heading_roles" / "v1" / "grammar.gbnf"
)

# The `role` rule of the heading_roles v1 grammar, in its order.
ROLES = (
    "chapter_heading",
    "part_heading",
    "section_heading",
    "subsection_heading",
    "running_head",
    "epigraph",
    "body",
    "caption",
    "other",
)

SEED = 20260923
ROMAN = ("I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X")

Template = Callable[[random.Random], str]

DE_TITLES = (
    "Die Reise nach Norden",
    "Methoden und Material",
    "Ergebnisse",
    "Der Brief",
    "Heimkehr",
    "Die Stadt am Fluss",
    "Vorbemerkungen",
    "Das Erbe",
    "Zur Frage der Übersetzung",
    "Grundbegriffe",
    "Die letzten Tage",
    "Quellen und Überlieferung",
)
DE_ORDINALS = ("Erstes", "Zweites", "Drittes", "Viertes", "Fünftes", "Sechstes")
DE_PART_ORDINALS = ("Erster", "Zweiter", "Dritter", "Vierter")
DE_BODY = (
    "Als er am nächsten Morgen erwachte, lag der Schnee schon hoch vor der Tür, und niemand im "
    "Haus wagte es, den Weg zum Dorf hinunterzugehen.",
    "Die Untersuchung stützt sich auf zweihundert Briefe, die zwischen 1820 und 1850 geschrieben "
    "und bisher nur zum Teil veröffentlicht wurden.",
    "Sie wusste, dass der Vater die Entscheidung längst getroffen hatte, und doch wartete sie bis "
    "zum Abend, bevor sie ihn danach fragte.",
    "In diesem Abschnitt werden die Verfahren beschrieben, mit denen die Proben gesammelt, "
    "aufbewahrt und schließlich im Labor ausgewertet wurden.",
    "Der Zug hielt an jeder kleinen Station, und an jeder stiegen dieselben müden Menschen aus, "
    "als hätte die Reise nie ein Ende.",
    "Niemand hatte damit gerechnet, dass der Winter in diesem Jahr so früh über die Berge kommen "
    "würde.",
    "Die Ergebnisse zeigen, dass die Unterschiede zwischen den beiden Gruppen kleiner sind, als "
    "frühere Studien vermuten ließen.",
    "Am Ende des Sommers verkaufte die Familie das Haus und zog in eine kleine Wohnung am Rand der "
    "Stadt.",
    "Für die vorliegende Ausgabe wurde der Text mit der Handschrift verglichen und an mehreren "
    "Stellen berichtigt.",
    "Er las den Brief zweimal, legte ihn dann in die Schublade und sprach nie wieder davon.",
    "Diese Annahme lässt sich nur halten, wenn man die wirtschaftlichen Bedingungen der Zeit außer "
    "Acht lässt.",
    "Draußen regnete es seit Stunden, und die Straßen der Altstadt lagen still und leer im Licht "
    "der Laternen.",
)
DE_QUOTES = (
    ("Was du ererbt von deinen Vätern hast, erwirb es, um es zu besitzen.", "Goethe"),
    ("Die Grenzen meiner Sprache bedeuten die Grenzen meiner Welt.", "Wittgenstein"),
    ("Es irrt der Mensch, solang er strebt.", "Goethe"),
    ("Ohne Musik wäre das Leben ein Irrtum.", "Nietzsche"),
    ("Wovon man nicht sprechen kann, darüber muss man schweigen.", "Wittgenstein"),
    ("Der Mensch ist nur da ganz Mensch, wo er spielt.", "Schiller"),
    ("Habe Mut, dich deines eigenen Verstandes zu bedienen!", "Kant"),
    ("Zwei Seelen wohnen, ach! in meiner Brust.", "Goethe"),
    ("Ich denke, also bin ich.", "Descartes"),
    ("Die Kunst ist eine Tochter der Freiheit.", "Schiller"),
    ("Aller Anfang ist schwer.", "Sprichwort"),
)
DE_CAPTION_SUBJECTS = (
    "Karte des Rheintals um 1850",
    "Anteil der Stichproben nach Region",
    "Titelblatt der Erstausgabe",
    "Verlauf der Temperatur im Januar",
    "Grundriss des Klosters",
    "Übersicht der verwendeten Quellen",
)
DE_OTHER: tuple[Template, ...] = (
    lambda r: f"ISBN 978-3-{r.randint(100, 999)}-{r.randint(10000, 99999)}-{r.randint(0, 9)}",
    lambda r: f"© {r.randint(1990, 2025)} Suhrkamp Verlag. Alle Rechte vorbehalten.",
    lambda r: (
        f"Druck und Bindung: Druckerei Friedrich Pustet, {r.choice(('Regensburg', 'Leipzig'))}"
    ),
    lambda r: "Gedruckt auf alterungsbeständigem Papier.",
)

TR_TITLES = (
    "Kuzeye Yolculuk",
    "Yöntem ve Gereç",
    "Bulgular",
    "Mektup",
    "Eve Dönüş",
    "Nehir Kıyısındaki Şehir",
    "Ön Notlar",
    "Miras",
    "Çeviri Sorunu Üzerine",
    "Temel Kavramlar",
    "Son Günler",
    "Kaynaklar ve Aktarım",
)
TR_ORDINALS = ("Birinci", "İkinci", "Üçüncü", "Dördüncü", "Beşinci", "Altıncı")
TR_BODY = (
    "Ertesi sabah uyandığında kar kapının önünde çoktan birikmişti ve evdekilerin hiçbiri köye "
    "inen yola çıkmaya cesaret edemedi.",
    "Bu çalışma, 1820 ile 1850 yılları arasında yazılmış ve şimdiye kadar yalnızca bir kısmı "
    "yayımlanmış iki yüz mektuba dayanmaktadır.",
    "Babasının kararı çoktan verdiğini biliyordu, yine de ona sormadan önce akşama kadar bekledi.",
    "Bu bölümde örneklerin toplanması, saklanması ve sonunda laboratuvarda incelenmesi için "
    "kullanılan yöntemler anlatılmaktadır.",
    "Tren her küçük istasyonda durdu ve her birinde aynı yorgun insanlar indi, sanki yolculuğun "
    "hiç sonu gelmeyecekti.",
    "O yıl kışın dağların üzerine bu kadar erken geleceğini kimse beklemiyordu.",
    "Sonuçlar, iki grup arasındaki farkların önceki çalışmaların öne sürdüğünden daha küçük "
    "olduğunu göstermektedir.",
    "Yazın sonunda aile evi sattı ve şehrin kenarındaki küçük bir daireye taşındı.",
    "Bu baskı için metin el yazmasıyla karşılaştırılmış ve birçok yerde düzeltilmiştir.",
    "Mektubu iki kez okudu, sonra çekmeceye koydu ve bir daha hiç bundan söz etmedi.",
    "Bu varsayım ancak dönemin ekonomik koşulları göz ardı edildiğinde savunulabilir.",
    "Dışarıda saatlerdir yağmur yağıyordu ve eski şehrin sokakları fenerlerin ışığında sessiz ve "
    "boş duruyordu.",
)
TR_QUOTES = (
    ("Hayatta en hakiki mürşit ilimdir.", "Atatürk"),
    ("Yaşamak bir ağaç gibi tek ve hür ve bir orman gibi kardeşçesine.", "Nâzım Hikmet"),
    ("Dinle neyden kim hikâyet etmede.", "Mevlânâ"),
    ("Ne içindeyim zamanın, ne de büsbütün dışında.", "Ahmet Hamdi Tanpınar"),
    ("Yurtta sulh, cihanda sulh.", "Atatürk"),
    ("Gel, gel, ne olursan ol yine gel.", "Mevlânâ"),
    ("İstanbul'u dinliyorum, gözlerim kapalı.", "Orhan Veli"),
    ("Beni bu güzel havalar mahvetti.", "Orhan Veli"),
    ("Damlaya damlaya göl olur.", "Atasözü"),
    ("Sakla samanı, gelir zamanı.", "Atasözü"),
    ("Ağlarsa anam ağlar, gerisi yalan ağlar.", "Atasözü"),
)
TR_CAPTION_SUBJECTS = (
    "1850 civarında Ren vadisinin haritası",
    "Bölgelere göre örneklem oranı",
    "İlk baskının kapak sayfası",
    "Ocak ayında sıcaklığın seyri",
    "Manastırın zemin planı",
    "Kullanılan kaynakların özeti",
)
TR_OTHER: tuple[Template, ...] = (
    lambda r: f"ISBN 978-605-{r.randint(100, 999)}-{r.randint(100, 999)}-{r.randint(0, 9)}",
    lambda r: f"© {r.randint(1990, 2025)} Yapı Kredi Yayınları. Tüm hakları saklıdır.",
    lambda r: f"Baskı ve cilt: {r.choice(('Ankara', 'İstanbul'))} Matbaacılık",
    lambda r: "Bu kitap asitsiz kâğıda basılmıştır.",
)


def _constant(text: str) -> Template:
    return lambda _r: text


def _quoted(quote: str, author: str, opening: str, closing: str) -> Template:
    return lambda _r: f"{opening}{quote}{closing} — {author}"


def _templates(lang: str) -> dict[str, tuple[Template, ...]]:
    if lang == "de":
        titles, ordinals, body, quotes, subjects, other = (
            DE_TITLES,
            DE_ORDINALS,
            DE_BODY,
            DE_QUOTES,
            DE_CAPTION_SUBJECTS,
            DE_OTHER,
        )
        chapter: tuple[Template, ...] = (
            lambda r: f"Kapitel {r.randint(1, 30)}",
            lambda r: f"{r.choice(ordinals)} Kapitel",
            lambda r: f"Kapitel {r.randint(1, 30)}: {r.choice(titles)}",
        )
        part: tuple[Template, ...] = (
            lambda r: f"Teil {r.choice(ROMAN)}",
            lambda r: f"{r.choice(DE_PART_ORDINALS)} Teil",
            lambda r: f"Buch {r.choice(ROMAN)}",
        )
        caption: tuple[Template, ...] = (
            lambda r: f"Abbildung {r.randint(1, 40)}: {r.choice(subjects)}",
            lambda r: f"Tabelle {r.randint(1, 20)}: {r.choice(subjects)}",
            lambda r: f"Abb. {r.randint(1, 9)}.{r.randint(1, 9)} {r.choice(subjects)}",
        )
        epigraph = tuple(_quoted(q, a, "„", "“") for q, a in quotes)
    else:
        titles, ordinals, body, quotes, subjects, other = (
            TR_TITLES,
            TR_ORDINALS,
            TR_BODY,
            TR_QUOTES,
            TR_CAPTION_SUBJECTS,
            TR_OTHER,
        )
        chapter = (
            lambda r: f"Bölüm {r.randint(1, 30)}",
            lambda r: f"{r.choice(ordinals)} Bölüm",
            lambda r: f"Bölüm {r.randint(1, 30)}: {r.choice(titles)}",
        )
        part = (
            lambda r: f"Kısım {r.choice(ROMAN)}",
            lambda r: f"{r.choice(ordinals)} Kısım",
            lambda r: f"Kitap {r.choice(ROMAN)}",
        )
        caption = (
            lambda r: f"Şekil {r.randint(1, 40)}: {r.choice(subjects)}",
            lambda r: f"Tablo {r.randint(1, 20)}: {r.choice(subjects)}",
            lambda r: f"Şek. {r.randint(1, 9)}.{r.randint(1, 9)} {r.choice(subjects)}",
        )
        epigraph = tuple(_quoted(q, a, "“", "”") for q, a in quotes)

    return {
        "chapter_heading": chapter,
        "part_heading": part,
        "section_heading": (lambda r: f"{r.randint(1, 12)}.{r.randint(1, 9)} {r.choice(titles)}",),
        "subsection_heading": (
            lambda r: f"{r.randint(1, 12)}.{r.randint(1, 9)}.{r.randint(1, 9)} {r.choice(titles)}",
        ),
        "running_head": (
            lambda r: f"{r.randint(10, 400)}    {r.choice(titles)}",
            lambda r: f"{r.choice(titles)}    {r.randint(10, 400)}",
        ),
        "epigraph": epigraph,
        "body": tuple(_constant(b) for b in body),
        "caption": caption,
        "other": other,
    }


def generate(lang: str) -> list[dict[str, str]]:
    """`model_gate.probe_items_per_language` probes for `lang`, the roles in turn."""
    count = int(thresholds.value("model_gate.probe_items_per_language"))
    rng = random.Random(f"{SEED}-{lang}")
    pools = _templates(lang)
    items: list[dict[str, str]] = []
    seen: set[str] = set()
    attempts = 0
    while len(items) < count:
        role = ROLES[len(items) % len(ROLES)]
        text = rng.choice(pools[role])(rng)
        attempts += 1
        # Duplicates are allowed only once a pool is exhausted, so a small pool (body) repeats
        # rather than the generator looping forever.
        if text in seen and attempts < count * 20:
            continue
        seen.add(text)
        items.append({"id": f"{lang}-{len(items):03d}", "lang": lang, "text": text, "role": role})
    return items


def path_for(lang: str) -> Path:
    return PROBES_DIR / f"{lang}_100.jsonl"


def dumps(items: list[dict[str, str]]) -> str:
    return "".join(json.dumps(item, ensure_ascii=False) + "\n" for item in items)


def load(lang: str) -> list[dict[str, str]]:
    return [json.loads(line) for line in path_for(lang).read_text(encoding="utf-8").splitlines()]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="rewrite the committed probe files")
    args = parser.parse_args(argv)
    for lang in ("de", "tr"):
        text = dumps(generate(lang))
        if args.write:
            path_for(lang).write_text(text, encoding="utf-8")
        print(f"{lang}: {len(text.splitlines())} probes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
