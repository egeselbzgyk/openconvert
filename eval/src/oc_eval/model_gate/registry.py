"""`model_gate.py --emit-registry`: fill `models.toml`'s pins from the hub (IMPLEMENTATION_PLAN
§1.6 fill rule, PHASE 9 detail 8), and the rule that keeps the default where D9 put it.

For each entry with a `TODO_` pin: resolve the repository's current commit
(`/api/models/<repo>` → `sha`), list that commit's tree and **assert the file is in it** (V1 §1(g):
the analogous Qwen3.5 repository answered 401 and does not exist), then download the file at that
commit and hash it while it streams. The hash written is the one *we* produced, and it must agree
with the hub's own LFS object id and size, or nothing is written.
"""

from __future__ import annotations

import hashlib
import re
import tomllib
import urllib.request
from pathlib import Path
from typing import Any, Protocol

from oc_eval.corpus import http

REPO_ROOT = Path(__file__).resolve().parents[4]
MODELS_TOML = REPO_ROOT / "models.toml"
HUB = "https://huggingface.co"
PLACEHOLDER = "TODO_"
DEFAULT_MODEL = "qwen3-1.7b-q4_k_m"
CHUNK = 1 << 20


class RegistryError(RuntimeError):
    pass


class Hub(Protocol):
    def json(self, url: str) -> Any: ...
    def sha256(self, url: str) -> tuple[str, int]: ...


class HttpHub:
    """The real hub. `json` goes through `oc_eval.corpus.http`; the download streams, because a
    model is gigabytes and hashing it must not need it in memory."""

    def json(self, url: str) -> Any:
        return http.get_json(url)

    def sha256(self, url: str) -> tuple[str, int]:
        if not url.startswith(f"{HUB}/"):
            raise RegistryError(f"refusing to download from {url!r}")
        request = urllib.request.Request(url, headers={"User-Agent": http.USER_AGENT})  # noqa: S310
        digest, size = hashlib.sha256(), 0
        with urllib.request.urlopen(request, timeout=http.TIMEOUT_SECONDS) as response:  # noqa: S310
            while chunk := response.read(CHUNK):
                digest.update(chunk)
                size += len(chunk)
        return digest.hexdigest(), size


def load(path: Path = MODELS_TOML) -> dict[str, Any]:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def entry(models: dict[str, Any], model_id: str) -> dict[str, Any]:
    for model in models["model"]:
        if model["id"] == model_id:
            return dict(model)
    raise RegistryError(f"{model_id!r} is not in the registry")


def resolve(model: dict[str, Any], hub: Hub) -> dict[str, Any]:
    repo, file = model["repo"], model["file"]
    commit = hub.json(f"{HUB}/api/models/{repo}")["sha"]
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise RegistryError(f"{repo}: the hub's sha {commit!r} is not a commit")
    tree = hub.json(f"{HUB}/api/models/{repo}/tree/{commit}")
    listed = next((item for item in tree if item.get("path") == file), None)
    if listed is None:
        raise RegistryError(f"{repo}@{commit} has no {file}")
    lfs = listed.get("lfs") or {}
    produced, size = hub.sha256(f"{HUB}/{repo}/resolve/{commit}/{file}")
    if lfs.get("oid") != produced or lfs.get("size", listed.get("size")) != size:
        raise RegistryError(
            f"{repo}/{file}: we hashed {produced} ({size} bytes); the hub lists "
            f"{lfs.get('oid')} ({lfs.get('size')} bytes)"
        )
    return {"revision": commit, "sha256": produced, "size_bytes": size}


def rewrite(text: str, model_id: str, fields: dict[str, Any]) -> str:
    """Set `fields` in the `[[model]]` block whose id is `model_id`, keeping everything else."""
    blocks = re.split(r"(?m)^(?=\[\[model\]\])", text)
    for i, block in enumerate(blocks):
        if not re.search(rf'(?m)^id\s*=\s*"{re.escape(model_id)}"', block):
            continue
        for key, value in fields.items():
            rendered = f'"{value}"' if isinstance(value, str) else str(value)
            block, n = re.subn(
                rf"(?m)^({key}\s*=\s*)(\"[^\"]*\"|\d+)([ \t]*#[^\n]*filled at Phase 9[^\n]*)?",
                lambda m, r=rendered: f"{m.group(1)}{r}",
                block,
            )
            if n != 1:
                raise RegistryError(f"{model_id}: expected one `{key}` line, found {n}")
        blocks[i] = block
        return "".join(blocks)
    raise RegistryError(f"{model_id!r} is not in the registry text")


def emit(path: Path, hub: Hub) -> list[str]:
    """Fill every unresolved entry in place. Returns the ids filled."""
    text = path.read_text(encoding="utf-8")
    filled = []
    for model in tomllib.loads(text)["model"]:
        if not any(str(model.get(k, "")).startswith(PLACEHOLDER) for k in ("revision", "sha256")):
            continue
        text = rewrite(text, model["id"], resolve(model, hub))
        filled.append(model["id"])
    path.write_text(text, encoding="utf-8")
    return filled


def default_problems(models: dict[str, Any], passed_on: dict[str, set[str]]) -> list[str]:
    """Row 9.20. `models.toml`'s `default` names a `tier = "default"` entry, and anything other
    than D9's Qwen3-1.7B needs a passing run on both reference machines. `passed_on` maps a
    model id to the machines on which its latest recorded run passed all nine gates."""
    default = models.get("default")
    problems = []
    tiers = {m["id"]: m.get("tier") for m in models["model"]}
    if tiers.get(default) != "default":
        problems.append(f'default {default!r} is not a tier = "default" entry')
    if default != DEFAULT_MODEL and not {"L", "M"} <= passed_on.get(str(default), set()):
        problems.append(
            f"default {default!r} replaces {DEFAULT_MODEL!r} without all nine gates green on "
            "machines L and M (docs/MODEL_GATE.md)"
        )
    return problems
