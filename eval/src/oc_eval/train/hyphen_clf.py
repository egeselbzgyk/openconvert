"""Train the tiny hyphenation classifier `oc-text` links in (PLAN Phase 3 detail 5, R2 §B.7).

The classifier decides what the four deterministic tiers leave open: a line-final hyphen whose
joined form is in no lexicon and whose halves are not both attested. That residual is where
dehyphenation goes wrong, and the numbers are the reason this script exists at all.

**Raw accuracy is the wrong metric here and will mislead anyone who reads it.** About 98 % of
line-break hyphens should simply be removed, so a model that always says "join" scores 98.8 %
and has learned nothing useful. The number that matters is **recall on "keep the hyphen"** —
how often a genuinely hyphenated word survives — and R2 §B.7 measures a dictionary-only
baseline at **31.7 %** against this kind of classifier's **85.8 %** (balanced accuracy 66.87 %
→ 92.38 %, over 776,700 hyphenated words). At 31.7 % keep-recall a 300-page novel with ~2,000
hyphenated line breaks produces hundreds of silently corrupted words while every conservation
check in the pipeline reports green (RT A1).

So the training objective is balanced, the reported metric is keep-recall, and the gate in
`crates/oc-text` is keep-recall on a held-out set of types this script never trained on.

Run it::

    cd eval && PYTHONPATH=src python -m oc_eval.train.hyphen_clf \\
        --out ../crates/oc-text/src/dehyphen --holdout data/hyphen_holdout.jsonl

Outputs, all committed:

* ``crates/oc-text/src/dehyphen/model.bin`` — the weights, in the format below.
* ``crates/oc-text/src/dehyphen/model.sources.json`` — the training manifest: every source
  read, its licence, its SHA-256, and the counts and metrics of the fit.
* ``eval/data/hyphen_holdout.jsonl`` — the held-out set the Rust gate scores against.

**Where the data comes from.** The same twelve CC0 Standard Ebooks the word-frequency list is
built from (D15, `oc_eval.generate.wordfreq`), because the licence question is already settled
for them and a training set is as much a shipped artefact as a word list is — the weights are
derived from it.

* **Keep** examples are the corpus's genuinely hyphenated types, split at their own hyphen:
  ``well-known`` → ``("well", "known")``. These are the words a wrong join destroys.
* **Join** examples are ordinary types split at an interior point: ``pipeline`` →
  ``("pipe", "line")``. The split point is chosen from a seeded generator rather than from
  hyphenation patterns, because the patterns are not ours to redistribute (VD-b) — an
  approximation, and one that costs the model only the finer grain of *where* a typesetter
  would have broken, not *whether* a break is a break.

The blob format, little-endian, deliberately dull for the same reason the frequency list's is:

    offset  size        meaning
    0       8           b"OCHYPH1\\0"
    8       4           bucket count B
    12      4           float32 bias
    16      4 * B       float32 weights

A feature is a short string; its bucket is ``fnv1a32(feature) % B``. Both sides compute it the
same way and `crates/oc-text/src/dehyphen/classifier.rs` says so in the same words.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import random
import re
import struct
import sys
import tarfile
import unicodedata
import urllib.request
from dataclasses import dataclass
from pathlib import Path

import numpy as np

MAGIC = b"OCHYPH1\0"

#: How many weight buckets. 8192 float32 is 32 KB, which is the "≈ 30 KB" the plan budgets,
#: and enough that character trigram features collide rarely at this vocabulary size.
BUCKETS = 8192

#: The seed for every random choice here. A training run that cannot be reproduced bit for bit
#: is a weight file nobody can audit.
SEED = 20260913

#: Held-out items, per the plan's test 3.12 ("a 2,000-item held-out set").
HOLDOUT_SIZE = 2000

#: A type has to occur at least this often to be evidence rather than a scanno.
MIN_COUNT = 2

#: The shortest piece either side of a break. Below two characters the features are mostly
#: absent and the example teaches the model about padding rather than about language.
MIN_PIECE = 2

_TAG = re.compile(r"<[^>]+>")
_ENTITY = re.compile(r"&(?:#\d+|#x[0-9a-fA-F]+|[a-zA-Z]+);")
_WORD = re.compile(r"[^\W\d_]+(?:['’\-][^\W\d_]+)*", re.UNICODE)


@dataclass(frozen=True)
class Source:
    name: str
    url: str
    license: str


#: The same twelve books `oc_eval.generate.wordfreq` builds the English list from. Adding one
#: is a licence decision (D15) before it is a code change.
SOURCES: tuple[Source, ...] = tuple(
    Source(
        name=repo,
        url=f"https://codeload.github.com/standardebooks/{repo}/tar.gz/refs/heads/master",
        license="CC0-1.0",
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
)

ALLOWED_LICENSES = frozenset({"CC0-1.0", "PD-US", "PD"})


def fnv1a32(text: str) -> int:
    """FNV-1a over UTF-8 bytes. Chosen because it is four lines in any language and the Rust
    side has to compute exactly the same number; a hash nobody can reimplement by eye is a
    hash that will one day disagree with itself across a version bump."""
    digest = 0x811C9DC5
    for byte in text.encode("utf-8"):
        digest ^= byte
        digest = (digest * 0x01000193) & 0xFFFFFFFF
    return digest


def features(head: str, tail: str, lang: str) -> list[str]:
    """The feature strings for one break.

    Character shape either side, case shape, bucketed lengths, and the language. Kept short
    and literal so that `classifier.rs` can be read next to this function and checked.
    """
    out = [
        "bias",
        f"lang={lang}",
        f"hl={min(len(head), 9)}",
        f"tl={min(len(tail), 9)}",
        f"hup={head[:1].isupper()}",
        f"tup={tail[:1].isupper()}",
        f"hyp={'-' in head or '-' in tail}",
    ]
    for n in (1, 2, 3):
        out.append(f"h{n}={head[-n:].lower()}")
        out.append(f"t{n}={tail[:n].lower()}")
    out.append(f"j={head[-1:].lower()}|{tail[:1].lower()}")
    return out


def vectorise(rows: list[tuple[str, str, str]]) -> np.ndarray:
    matrix = np.zeros((len(rows), BUCKETS), dtype=np.float32)
    for index, (head, tail, lang) in enumerate(rows):
        for feature in features(head, tail, lang):
            matrix[index, fnv1a32(feature) % BUCKETS] += 1.0
    return matrix


def fetch(source: Source, cache: Path) -> bytes:
    cache.mkdir(parents=True, exist_ok=True)
    target = cache / f"{source.name}.tar.gz"
    if target.exists():
        return target.read_bytes()
    with urllib.request.urlopen(source.url, timeout=180) as response:  # noqa: S310
        payload = response.read()
    target.write_bytes(payload)
    return payload


def text_of(xhtml: str) -> str:
    return _ENTITY.sub(" ", _TAG.sub(" ", xhtml))


def count_types(cache: Path) -> tuple[dict[str, int], list[dict[str, object]]]:
    """Count word types across the corpus, and record what was read."""
    counts: dict[str, int] = {}
    manifest: list[dict[str, object]] = []
    for source in SOURCES:
        if source.license not in ALLOWED_LICENSES:
            raise SystemExit(f"{source.name}: licence {source.license} is not allow-listed")
        payload = fetch(source, cache)
        files = 0
        with tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz") as archive:
            for member in archive:
                if not member.isfile() or "src/epub/text/" not in member.name:
                    continue
                handle = archive.extractfile(member)
                if handle is None:
                    continue
                files += 1
                text = text_of(handle.read().decode("utf-8", "replace"))
                for match in _WORD.finditer(text):
                    word = unicodedata.normalize("NFC", match.group(0)).lower()
                    counts[word] = counts.get(word, 0) + 1
        manifest.append(
            {
                "name": source.name,
                "url": source.url,
                "license": source.license,
                "sha256": hashlib.sha256(payload).hexdigest(),
                "files_read": files,
            }
        )
    return counts, manifest


def build_examples(counts: dict[str, int]) -> list[dict[str, object]]:
    """Turn the type counts into labelled breaks.

    One example per *type*, never per token: a corpus's hundred occurrences of `to-day` are
    one fact about English, and weighting by frequency would train the model on Dickens's
    punctuation habits.
    """
    rng = random.Random(SEED)
    examples: list[dict[str, object]] = []
    for word, count in sorted(counts.items()):
        if count < MIN_COUNT or "'" in word or "’" in word:
            continue
        if "-" in word:
            seam = word.find("-")
            head, tail = word[:seam], word[seam + 1 :]
            if len(head) < MIN_PIECE or len(tail) < MIN_PIECE or "-" in tail[:1]:
                continue
            examples.append({"head": head, "tail": tail, "lang": "en", "keep": True})
        else:
            if len(word) < MIN_PIECE * 2:
                continue
            seam = rng.randrange(MIN_PIECE, len(word) - MIN_PIECE + 1)
            examples.append({"head": word[:seam], "tail": word[seam:], "lang": "en", "keep": False})
    rng.shuffle(examples)
    return examples


def train(matrix: np.ndarray, labels: np.ndarray, weights: np.ndarray) -> tuple[np.ndarray, float]:
    """Logistic regression by gradient descent, with class weights and L2.

    Eighty lines of linfa would do this too; the plan says not to add the dependency, and for
    a convex problem in eight thousand dimensions a fixed schedule is entirely adequate.
    """
    rng = np.random.default_rng(SEED)
    theta: np.ndarray = np.asarray(rng.normal(0.0, 0.01, size=matrix.shape[1]), dtype=np.float64)
    bias = 0.0
    rate = 0.5
    l2 = 1e-5
    for epoch in range(400):
        scores = matrix @ theta + bias
        probability = 1.0 / (1.0 + np.exp(-scores))
        error = (probability - labels) * weights
        gradient = matrix.T @ error / matrix.shape[0] + l2 * theta
        theta -= rate * gradient
        bias -= rate * float(error.mean())
        if epoch == 250:
            rate *= 0.3
    return theta, bias


def metrics(scores: np.ndarray, labels: np.ndarray) -> dict[str, float]:
    predicted_keep = scores > 0.0
    actual_keep = labels > 0.5
    keep_total = max(int(actual_keep.sum()), 1)
    join_total = max(int((~actual_keep).sum()), 1)
    keep_recall = float(int((predicted_keep & actual_keep).sum()) / keep_total)
    join_recall = float(int(((~predicted_keep) & (~actual_keep)).sum()) / join_total)
    return {
        "keep_recall": keep_recall,
        "join_recall": join_recall,
        "balanced_accuracy": (keep_recall + join_recall) / 2.0,
        "accuracy": float((predicted_keep == actual_keep).mean()),
    }


def write_model(path: Path, theta: np.ndarray, bias: float) -> None:
    blob = bytearray()
    blob += MAGIC
    blob += struct.pack("<I", BUCKETS)
    blob += struct.pack("<f", bias)
    blob += struct.pack(f"<{BUCKETS}f", *theta.astype(np.float32).tolist())
    path.write_bytes(bytes(blob))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path("../crates/oc-text/src/dehyphen"))
    parser.add_argument("--holdout", type=Path, default=Path("data/hyphen_holdout.jsonl"))
    parser.add_argument(
        "--cache",
        type=Path,
        default=Path.home() / ".cache" / "openconvert" / "wordfreq",
        help="where the downloaded sources live; outside the repository on purpose",
    )
    args = parser.parse_args(argv)

    counts, manifest = count_types(args.cache)
    examples = build_examples(counts)
    keeps = [example for example in examples if example["keep"]]
    joins = [example for example in examples if not example["keep"]]
    if len(keeps) < HOLDOUT_SIZE // 4:
        raise SystemExit(f"only {len(keeps)} keep examples; the corpus is too small to gate on")

    # The holdout is stratified, so keep-recall is measured on enough keeps to mean something,
    # and it is split by *type*: no word appears on both sides.
    rng = random.Random(SEED + 1)
    rng.shuffle(keeps)
    rng.shuffle(joins)
    # A quarter of the keeps, with a floor so the recall estimate is not noise: at n = 100 a
    # 95 % interval around 0.85 is about ±7 pp, which is enough to gate at 0.80 and not much
    # tighter than the population of hyphenated types in twelve novels can support anyway.
    keep_out = max(100, len(keeps) // 4)
    join_out = HOLDOUT_SIZE - keep_out
    holdout = keeps[:keep_out] + joins[:join_out]
    training = keeps[keep_out:] + joins[join_out:]
    rng.shuffle(holdout)
    rng.shuffle(training)

    rows = [(str(row["head"]), str(row["tail"]), str(row["lang"])) for row in training]
    matrix = vectorise(rows).astype(np.float64)
    labels = np.array([1.0 if row["keep"] else 0.0 for row in training])
    # Balanced class weights: the population is 98 % join, and a model fitted to that prior
    # predicts "join" and is right 98 % of the time while being useless.
    positive = labels.sum()
    negative = len(labels) - positive
    weights = np.where(labels > 0.5, negative / max(positive, 1.0), 1.0)

    theta, bias = train(matrix, labels, weights)

    holdout_rows = [(str(row["head"]), str(row["tail"]), str(row["lang"])) for row in holdout]
    holdout_scores = vectorise(holdout_rows).astype(np.float64) @ theta + bias
    holdout_labels = np.array([1.0 if row["keep"] else 0.0 for row in holdout])
    holdout_metrics = metrics(holdout_scores, holdout_labels)
    train_metrics = metrics(matrix @ theta + bias, labels)

    args.out.mkdir(parents=True, exist_ok=True)
    write_model(args.out / "model.bin", theta, bias)
    args.holdout.parent.mkdir(parents=True, exist_ok=True)
    with args.holdout.open("w", encoding="utf-8", newline="\n") as handle:
        for row in holdout:
            handle.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n")

    (args.out / "model.sources.json").write_text(
        json.dumps(
            {
                "format": MAGIC.decode("ascii").rstrip("\0"),
                "buckets": BUCKETS,
                "seed": SEED,
                "generator": "oc_eval.train.hyphen_clf",
                "sources": manifest,
                "counts": {
                    "types_seen": len(counts),
                    "training": len(training),
                    "training_keep": int(positive),
                    "holdout": len(holdout),
                    "holdout_keep": keep_out,
                },
                "metrics": {"training": train_metrics, "holdout": holdout_metrics},
                "note": (
                    "Keep-recall is the metric that matters; raw accuracy is misleading "
                    "because ~98 % of line-break hyphens should be removed (R2 §B.7)."
                ),
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
        newline="\n",
    )

    print(json.dumps(holdout_metrics, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
