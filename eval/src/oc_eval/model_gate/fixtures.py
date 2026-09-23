"""The 20 prompt fixtures G2–G5 ask and G3 tokenizes (D9 G2, G3).

Four are the Rust renderer's own output, read from the committed cassettes
(`crates/oc-ai/tests/cassettes/<task>/`), which are Appendix A.3's worked examples exactly as the
pipeline sends them. Sixteen more follow the same templates (`crates/oc-ai/prompts/<task>/v1/
user.tmpl`) and the renderer's compact JSON, in English, German and Turkish, so every production
grammar is asked five questions. Each fixture carries what G2's semantic assertions need: the ids an
answer must cover, and for `metadata` the payload an answer must copy from.

`reference_tokens` is `null` in every fixture. G3 compares the server's tokenization of the
rendered chat against the model's reference tokenizer, and filling it needs that tokenizer at the
pinned revision, which this repository cannot fetch from where it was built. G3 reports `not_run`
until it is filled, never a pass.

`python -m oc_eval.model_gate.fixtures --write` regenerates
`eval/data/probes/prompt_fixtures_20.jsonl`.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

from oc_eval import thresholds

REPO_ROOT = Path(__file__).resolve().parents[4]
PROMPTS = REPO_ROOT / "crates" / "oc-ai" / "prompts"
CASSETTES = REPO_ROOT / "crates" / "oc-ai" / "tests" / "cassettes"
PATH = REPO_ROOT / "eval" / "data" / "probes" / "prompt_fixtures_20.jsonl"
PURPOSES = ("metadata", "heading_roles", "book_structure", "verse_quote")


def compact(value: Any) -> str:
    """The renderer's JSON: no whitespace, UTF-8 as is (crates/oc-ai/src/prompt/render.rs)."""
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def template(purpose: str) -> str:
    return (PROMPTS / purpose / "v1" / "user.tmpl").read_text(encoding="utf-8")


def render(purpose: str, payload: Any) -> str:
    slot = "{{pages}}" if purpose == "metadata" else "{{payload}}"
    return template(purpose).replace(slot, payload if purpose == "metadata" else compact(payload))


def system_prompt() -> str:
    """The shared prefix, byte-identical across all four tasks (ARCHITECTURE §9.3)."""
    return (PROMPTS / "metadata" / "v1" / "system.md").read_text(encoding="utf-8")


def grammar(purpose: str) -> str:
    return (PROMPTS / purpose / "v1" / "grammar.gbnf").read_text(encoding="utf-8")


def _from_cassette(purpose: str) -> dict[str, Any]:
    [path] = [p for p in (CASSETTES / purpose).glob("*.json") if p.name != "index.json"]
    user = json.loads(path.read_text(encoding="utf-8"))["request"]["user"]
    fixture: dict[str, Any] = {"purpose": purpose, "user": user, "source": f"cassette:{purpose}"}
    body = user.split("\n", 1)[1] if purpose != "metadata" else user
    fixture["expect"] = _expect(purpose, body)
    return fixture


def _expect(purpose: str, body: str) -> dict[str, Any]:
    if purpose == "metadata":
        return {"payload": body}
    payload = json.loads(body.strip())
    if purpose == "heading_roles":
        return {
            "clusters": [c["c"] for c in payload["clusters"]],
            "holdout": [h["i"] for h in payload.get("holdout", [])],
        }
    if purpose == "book_structure":
        return {"headings": len(payload["headings"])}
    return {"blocks": [b["id"] for b in payload["blocks"]]}


def _metadata(lines: list[str]) -> str:
    return "\n".join(lines)


METADATA_PAGES = (
    [
        "[LARGE][CENTERED] The Grammar of Rivers",
        "[MEDIUM][CENTERED] A Natural History",
        "[SMALL][CENTERED] Margaret L. Hale",
        "[SMALL] Northfield Press · Boston · 1987",
    ],
    [
        "[LARGE][CENTERED] Der Nachsommer",
        "[MEDIUM][CENTERED] Eine Erzählung",
        "[SMALL][CENTERED] von Adalbert Stifter",
        "[SMALL] Heckenast · Pest · 1857",
    ],
    [
        "[LARGE][CENTERED] Kuyucaklı Yusuf",
        "[SMALL][CENTERED] Sabahattin Ali",
        "[SMALL] Remzi Kitabevi · İstanbul · 1937",
    ],
    [
        "[LARGE][CENTERED] On Translation",
        "[MEDIUM][CENTERED] Essays",
        "[SMALL][CENTERED] translated by Paul Brandt",
        "[SMALL] 2019",
    ],
)

HEADING_ROLES = (
    {
        "language": "en",
        "clusters": [
            {
                "c": 0,
                "size_z": 2.8,
                "weight": "bold",
                "italic": False,
                "align": "left",
                "count": 12,
                "starts_page_ratio": 0.92,
                "examples": ["Chapter One", "Chapter Two"],
            },
            {
                "c": 1,
                "size_z": 0.0,
                "weight": "regular",
                "italic": False,
                "align": "justified",
                "count": 2210,
                "starts_page_ratio": 0.01,
                "examples": ["It was late in the year when"],
            },
        ],
        "holdout": [
            {
                "i": 0,
                "text": "Chapter Three",
                "size_z": 2.8,
                "weight": "bold",
                "italic": False,
                "align": "left",
            }
        ],
    },
    {
        "language": "tr",
        "clusters": [
            {
                "c": 0,
                "size_z": 3.0,
                "weight": "bold",
                "italic": False,
                "align": "centered",
                "count": 18,
                "starts_page_ratio": 0.95,
                "examples": ["Birinci Bölüm", "İkinci Bölüm"],
            },
            {
                "c": 1,
                "size_z": 1.2,
                "weight": "bold",
                "italic": False,
                "align": "left",
                "count": 64,
                "starts_page_ratio": 0.1,
                "examples": ["1.1 Giriş", "1.2 Yöntem"],
            },
            {
                "c": 2,
                "size_z": 0.0,
                "weight": "regular",
                "italic": False,
                "align": "justified",
                "count": 3100,
                "starts_page_ratio": 0.02,
                "examples": ["Bu çalışmada ele alınan"],
            },
        ],
        "holdout": [
            {
                "i": 0,
                "text": "2.3 Bulgular",
                "size_z": 1.2,
                "weight": "bold",
                "italic": False,
                "align": "left",
            }
        ],
    },
    {
        "language": "de",
        "clusters": [
            {
                "c": 0,
                "size_z": 3.4,
                "weight": "bold",
                "italic": False,
                "align": "centered",
                "count": 3,
                "starts_page_ratio": 1.0,
                "examples": ["Erster Teil", "Zweiter Teil"],
            },
            {
                "c": 1,
                "size_z": 2.1,
                "weight": "bold",
                "italic": False,
                "align": "centered",
                "count": 21,
                "starts_page_ratio": 0.9,
                "examples": ["I.", "II."],
            },
            {
                "c": 2,
                "size_z": 0.0,
                "weight": "regular",
                "italic": False,
                "align": "justified",
                "count": 4020,
                "starts_page_ratio": 0.01,
                "examples": ["Mein Vater war ein Kaufmann"],
            },
            {
                "c": 3,
                "size_z": -1.1,
                "weight": "regular",
                "italic": True,
                "align": "centered",
                "count": 14,
                "starts_page_ratio": 0.0,
                "examples": ["Abb. 1 Das Haus am Hügel"],
            },
        ],
        "holdout": [],
    },
    {
        "language": "en",
        "clusters": [
            {
                "c": 0,
                "size_z": 0.2,
                "weight": "regular",
                "italic": True,
                "align": "right",
                "count": 9,
                "starts_page_ratio": 0.9,
                "examples": ["The heart has its reasons — Pascal"],
            },
            {
                "c": 1,
                "size_z": 0.0,
                "weight": "regular",
                "italic": False,
                "align": "justified",
                "count": 1500,
                "starts_page_ratio": 0.02,
                "examples": ["The house stood at the end"],
            },
        ],
        "holdout": [],
    },
)

BOOK_STRUCTURE = (
    {
        "language": "en",
        "headings": [
            {"idx": 0, "text": "Preface", "page": 5, "c": 1},
            {"idx": 1, "text": "Part One", "page": 9, "c": 2},
            {"idx": 2, "text": "Chapter 1", "page": 11, "c": 0},
            {"idx": 3, "text": "Part Two", "page": 120, "c": 2},
            {"idx": 4, "text": "Chapter 2", "page": 122, "c": 0},
            {"idx": 5, "text": "Notes", "page": 240, "c": 1},
        ],
    },
    {
        "language": "de",
        "headings": [
            {"idx": 0, "text": "Kapitel 1", "page": 1, "c": 0},
            {"idx": 1, "text": "Kapitel 2", "page": 30, "c": 0},
            {"idx": 2, "text": "Kapitel 3", "page": 61, "c": 0},
        ],
    },
    {
        "language": "tr",
        "headings": [
            {"idx": 0, "text": "İçindekiler", "page": 2, "c": 1},
            {"idx": 1, "text": "Giriş", "page": 5, "c": 1},
            {"idx": 2, "text": "Birinci Kısım", "page": 9, "c": 2},
            {"idx": 3, "text": "Birinci Bölüm", "page": 11, "c": 0},
            {"idx": 4, "text": "Kaynakça", "page": 301, "c": 1},
        ],
    },
    {
        "language": "en",
        "headings": [
            {"idx": 0, "text": "Introduction", "page": 1, "c": 0},
            {"idx": 1, "text": "Methods", "page": 8, "c": 0},
            {"idx": 2, "text": "Appendix A", "page": 90, "c": 1},
            {"idx": 3, "text": "Index", "page": 99, "c": 1},
        ],
    },
)

VERSE_QUOTE = (
    {
        "blocks": [
            {
                "id": "AB3DEF7HJK",
                "text": "Ich weiß nicht, was soll es bedeuten,\nDass ich so traurig bin",
                "indent": "deep",
                "lines": 4,
                "avg_line_words": 6,
                "centered": False,
                "monospace": False,
            },
            {
                "id": "CD4EFG2HJM",
                "text": 'fn main() {\n    println!("hello");\n}',
                "indent": "deep",
                "lines": 3,
                "avg_line_words": 2,
                "centered": False,
                "monospace": True,
            },
        ]
    },
    {
        "blocks": [
            {
                "id": "EF5GHJ3KLN",
                "text": "Science is a way of thinking much more than it is a body "
                "of knowledge, and it was in that spirit that the society was founded.",
                "indent": "both",
                "lines": 3,
                "avg_line_words": 11,
                "centered": False,
                "monospace": False,
            }
        ]
    },
    {
        "blocks": [
            {
                "id": "GH6JKL4MNP",
                "text": "Dinle neyden kim hikâyet etmede\nAyrılıklardan şikâyet etmede",
                "indent": "deep",
                "lines": 2,
                "avg_line_words": 4,
                "centered": True,
                "monospace": False,
            },
            {
                "id": "JK7LMN5PQR",
                "text": "O gün öğleden sonra, rıhtımda beklerken, uzun zamandır "
                "görmediği bir arkadaşına rastladı.",
                "indent": "first",
                "lines": 2,
                "avg_line_words": 9,
                "centered": False,
                "monospace": False,
            },
        ]
    },
    {
        "blocks": [
            {
                "id": "LM2NPQ6RST",
                "text": "Tyger Tyger, burning bright,\nIn the forests of the night;\n"
                "What immortal hand or eye,\nCould frame thy fearful symmetry?",
                "indent": "deep",
                "lines": 4,
                "avg_line_words": 5,
                "centered": False,
                "monospace": False,
            }
        ]
    },
)


def generate() -> list[dict[str, Any]]:
    fixtures = [_from_cassette(purpose) for purpose in PURPOSES]
    composed: list[tuple[str, Any]] = [
        *[("metadata", _metadata(p)) for p in METADATA_PAGES],
        *[("heading_roles", p) for p in HEADING_ROLES],
        *[("book_structure", p) for p in BOOK_STRUCTURE],
        *[("verse_quote", p) for p in VERSE_QUOTE],
    ]
    for purpose, payload in composed:
        user = render(purpose, payload)
        body = payload if purpose == "metadata" else compact(payload)
        fixtures.append(
            {
                "purpose": purpose,
                "user": user,
                "source": "composed",
                "expect": _expect(purpose, body),
            }
        )
    count = int(thresholds.value("model_gate.g3_fixtures"))
    if len(fixtures) != count:
        raise ValueError(f"{len(fixtures)} fixtures, G3 asks for {count}")
    for index, fixture in enumerate(fixtures):
        fixture["id"] = f"f{index:02d}-{fixture['purpose']}"
        fixture["reference_tokens"] = None
    return [
        {key: f[key] for key in ("id", "purpose", "source", "user", "expect", "reference_tokens")}
        for f in fixtures
    ]


def dumps(fixtures: list[dict[str, Any]]) -> str:
    return "".join(json.dumps(f, ensure_ascii=False) + "\n" for f in fixtures)


def load(path: Path = PATH) -> list[dict[str, Any]]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args(argv)
    text = dumps(generate())
    if args.write:
        PATH.write_text(text, encoding="utf-8")
    print(f"{len(text.splitlines())} fixtures")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
