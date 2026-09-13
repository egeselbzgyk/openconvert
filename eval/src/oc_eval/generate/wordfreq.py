"""Build the word-frequency lists `oc-text` links in (D15, RT B10, PLAN Phase 2 detail 5).

The lists answer one question — "is this a word in this language?" — for the dictionary hit
rate, which is one of the two independent signals that call a page `broken_text` (D13.10,
PIPELINE §4). They are *data*, so their provenance is as much a licence question as any
crate's, and this script is the record of it: it refuses to read a source that is not on the
allow-list in `SOURCES`, it writes a manifest naming every file it read, and the manifest is
committed next to the blob it produced.

**No hunspell, no igerman98, no `zspell`** (D15): igerman98 is GPL/LGPL and a hunspell
dictionary is a spell-checker's affix machinery, which is far more than a membership test and
carries the licence of whoever compiled it.

Run it::

    python -m oc_eval.generate.wordfreq en --out ../crates/oc-text/src/freq
    python -m oc_eval.generate.wordfreq de tr --out ../crates/oc-text/src/freq

Output per language: `<lang>.bin` (the blob `oc-text` reads) and `<lang>.sources.json`
(what it was built from). Both are committed.

The blob format, little-endian throughout, is deliberately dull — a sorted string table and
an offset index, binary-searched. No FST, no perfect hash, no crate: the whole operation is
"does this byte string appear in a sorted list", the list fits in a couple of megabytes, and
a format anyone can read with a hex editor is one nobody has to trust.

    offset  size            meaning
    0       8               b"OCFREQ1\\0"
    8       8               language tag, ASCII, NUL-padded (`en`, `de`, `tr`)
    16      4               word count N
    20      4 * (N + 1)     byte offsets into the word table; offsets[0] == 0
    ...     offsets[N]      the words, folded, sorted by byte order, concatenated

Words are stored **folded** — lower-cased in the language's own locale, so Turkish `I` is
`ı` and `İ` is `i` (R10 §6.3) — because that is the form the lookup key takes and folding at
build time is folding once instead of once per query.
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import io
import json
import re
import struct
import sys
import tarfile
import unicodedata
import urllib.request
from dataclasses import dataclass
from pathlib import Path

MAGIC = b"OCFREQ1\0"
LANG_FIELD_BYTES = 8

#: How many words a shipped list holds at most. PLAN Phase 2 detail 5 says "top-200 k"; a
#: source set that yields fewer simply yields fewer, and the manifest says so.
DEFAULT_TOP_N = 200_000

#: A word has to appear at least this often to be a word rather than a scanno or a name that
#: happens to occur once. Two is the smallest bar that removes hapax legomena, which are
#: roughly half the distinct forms in any natural corpus and almost none of its running text.
MIN_COUNT = 2

#: Tokens shorter than this are not evidence either way — `a`, `I`, and every stray letter a
#: broken CMap produces.
MIN_WORD_CHARS = 2


@dataclass(frozen=True)
class Source:
    """One corpus, and why it is allowed to be in a shipped artefact."""

    name: str
    url: str
    #: The licence of the *text*, which must be CC0 or public domain. A CC-BY-SA
    #: transcription of a public-domain work is **not** admissible here without a decision
    #: recorded in `docs/DECISIONS.md`: D15's allow-list governs shipped artefacts and
    #: CC-BY-SA is not on it.
    license: str
    #: Where inside the archive the text lives.
    member_glob: str


#: The allow-list. Adding a language means adding sources here, running the script, and
#: committing the blob with its manifest — and checking the licence at that moment, not
#: relying on this comment.
#:
#: **English** is Standard Ebooks, which dedicates its editions to the public domain under
#: CC0 (https://standardebooks.org/manual — "Standard Ebooks releases its ebooks into the
#: U.S. public domain"). The GitHub source repositories carry the same dedication and are
#: fetchable as a single tarball each, which is one request per book rather than one per
#: chapter.
#:
#: **German and Turkish are deliberately absent.** PLAN Phase 2 detail 5 names DTA plain text
#: and Wikisource-TR; both host public-domain *works* under CC-BY-SA *transcriptions*, which
#: is a different licence from the one this script requires and from the one D15 allows for a
#: shipped artefact. Resolving that is a D15 decision, not a script change. Until it is made,
#: `dict_hit_rate` returns `None` for those languages — which is not the same as zero, and the
#: rules that consume it abstain rather than call a German book broken.
SOURCES: dict[str, tuple[Source, ...]] = {
    "en": tuple(
        Source(
            name=repo,
            url=f"https://codeload.github.com/standardebooks/{repo}/tar.gz/refs/heads/master",
            license="CC0-1.0",
            member_glob="src/epub/text/",
        )
        for repo in (
            "herman-melville_moby-dick",
            "jane-austen_pride-and-prejudice",
            "charles-dickens_a-tale-of-two-cities",
            "mary-shelley_frankenstein",
            "lewis-carroll_alices-adventures-in-wonderland",
            "arthur-conan-doyle_the-adventures-of-sherlock-holmes",
            "bram-stoker_dracula",
            "charlotte-bronte_jane-eyre",
            "george-eliot_middlemarch",
            "mark-twain_the-adventures-of-tom-sawyer",
            "jonathan-swift_gullivers-travels",
            "h-g-wells_the-war-of-the-worlds",
        )
    ),
}

ALLOWED_LICENSES = frozenset({"CC0-1.0", "PD-US", "PD"})

_TAG = re.compile(r"<[^>]+>")
_ENTITY = re.compile(r"&(?:#\d+|#x[0-9a-fA-F]+|[a-zA-Z]+);")
#: A word is a run of letters, optionally joined by an apostrophe or a hyphen. Digits are not
#: words: a frequency list that contained `1996` would score a page of glyph indices as
#: partially recognised.
_WORD = re.compile(r"[^\W\d_]+(?:['’\-][^\W\d_]+)*", re.UNICODE)


def fold(word: str, lang: str) -> str:
    """Lower-case `word` the way `oc_text::fold::fold_key` does, and compose it.

    The two implementations have to agree exactly or every lookup misses. Turkish and
    Azerbaijani pair the dotted and dotless i their own way; everything else uses the
    default mapping.
    """
    composed = unicodedata.normalize("NFC", word)
    if lang.split("-")[0] in {"tr", "az"}:
        composed = composed.replace("İ", "i").replace("I", "ı")
    return unicodedata.normalize("NFC", composed.lower())


def text_of(xhtml: str) -> str:
    """Strip markup. Crude on purpose: a word list does not need a parser, and `lxml` on ten
    thousand chapter files costs more than the whitespace it saves."""
    without_tags = _TAG.sub(" ", xhtml)
    return _ENTITY.sub(" ", without_tags)


def fetch(source: Source, cache: Path) -> bytes:
    """Download a source, or reuse the cached copy. The cache is outside the repository."""
    cache.mkdir(parents=True, exist_ok=True)
    target = cache / f"{source.name}.tar.gz"
    if target.exists():
        return target.read_bytes()
    with urllib.request.urlopen(source.url, timeout=120) as response:  # noqa: S310
        payload = response.read()
    target.write_bytes(payload)
    return payload


def count_words(lang: str, sources: tuple[Source, ...], cache: Path) -> tuple[
    collections.Counter[str], list[dict[str, object]]
]:
    counts: collections.Counter[str] = collections.Counter()
    manifest: list[dict[str, object]] = []

    for source in sources:
        if source.license not in ALLOWED_LICENSES:
            raise SystemExit(
                f"{source.name} is {source.license}; this script builds shipped artefacts and "
                f"only reads {sorted(ALLOWED_LICENSES)}. See D15."
            )
        payload = fetch(source, cache)
        digest = hashlib.sha256(payload).hexdigest()
        files = 0
        with tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz") as archive:
            for member in archive:
                if not member.isfile() or source.member_glob not in member.name:
                    continue
                if not member.name.endswith((".xhtml", ".html", ".txt")):
                    continue
                handle = archive.extractfile(member)
                if handle is None:
                    continue
                body = handle.read().decode("utf-8", errors="replace")
                for word in _WORD.findall(text_of(body)):
                    folded = fold(word, lang)
                    if len(folded) >= MIN_WORD_CHARS:
                        counts[folded] += 1
                files += 1
        manifest.append(
            {
                "name": source.name,
                "url": source.url,
                "license": source.license,
                "sha256": digest,
                "files_read": files,
            }
        )
    return counts, manifest


def build_blob(lang: str, words: list[str]) -> bytes:
    """Serialise a sorted, deduplicated word list into the format `oc-text` reads."""
    table = bytearray()
    offsets = [0]
    for word in words:
        table.extend(word.encode("utf-8"))
        offsets.append(len(table))

    tag = lang.encode("ascii")
    if len(tag) > LANG_FIELD_BYTES:
        raise SystemExit(f"language tag {lang!r} does not fit in {LANG_FIELD_BYTES} bytes")

    out = bytearray()
    out.extend(MAGIC)
    out.extend(tag.ljust(LANG_FIELD_BYTES, b"\0"))
    out.extend(struct.pack("<I", len(words)))
    out.extend(struct.pack(f"<{len(offsets)}I", *offsets))
    out.extend(table)
    return bytes(out)


def build(lang: str, out_dir: Path, cache: Path, top_n: int) -> None:
    sources = SOURCES.get(lang)
    if not sources:
        raise SystemExit(
            f"no allow-listed source for {lang!r}. Adding one is a licence decision "
            f"(D15) before it is a code change; see the SOURCES comment."
        )

    counts, manifest = count_words(lang, sources, cache)
    kept = [word for word, count in counts.items() if count >= MIN_COUNT]
    kept.sort(key=lambda word: (-counts[word], word))
    kept = kept[:top_n]
    # Stored sorted by bytes, because the reader binary-searches it.
    kept.sort(key=lambda word: word.encode("utf-8"))

    out_dir.mkdir(parents=True, exist_ok=True)
    blob = build_blob(lang, kept)
    (out_dir / f"{lang}.bin").write_bytes(blob)
    (out_dir / f"{lang}.sources.json").write_text(
        json.dumps(
            {
                "lang": lang,
                "generator": "eval/src/oc_eval/generate/wordfreq.py",
                "word_count": len(kept),
                "min_count": MIN_COUNT,
                "min_word_chars": MIN_WORD_CHARS,
                "top_n": top_n,
                "blob_sha256": hashlib.sha256(blob).hexdigest(),
                "sources": manifest,
            },
            indent=2,
            ensure_ascii=False,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"{lang}: {len(kept)} words, {len(blob)} bytes", file=sys.stderr)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("langs", nargs="+", help="language tags to build, e.g. en de tr")
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("crates/oc-text/src/freq"),
        help="where the .bin and .sources.json land",
    )
    parser.add_argument(
        "--cache",
        type=Path,
        default=Path("target/wordfreq-cache"),
        help="download cache, outside the repository's committed tree",
    )
    parser.add_argument("--top-n", type=int, default=DEFAULT_TOP_N)
    args = parser.parse_args(argv)

    for lang in args.langs:
        build(lang, args.out, args.cache, args.top_n)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
