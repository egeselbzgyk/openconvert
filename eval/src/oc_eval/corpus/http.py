"""The one place in `oc_eval` that opens a URL.

Same rule as `download.py`: https only, a User-Agent that says who we are, and a timeout. The
harvest reads public catalogue APIs, and several of them refuse a request with no User-Agent —
arXiv answers 406 — so this is not decoration.
"""

from __future__ import annotations

import json
import time
import urllib.error
import urllib.request
from typing import Any

USER_AGENT = "openconvert-corpus/1 (+https://github.com/openconvert/openconvert)"
TIMEOUT_SECONDS = 90

# Catalogue APIs are free and someone else pays for them. One request a second is polite and
# still harvests a hundred documents in minutes.
POLITE_DELAY_SECONDS = 1.0

_last_request_at = 0.0


class HttpError(RuntimeError):
    """A request that did not come back with a body."""


def get_bytes(url: str, *, retries: int = 3) -> bytes:
    global _last_request_at

    if not url.startswith("https://"):
        raise HttpError(f"refusing a non-https URL: {url!r}")

    last: Exception | None = None
    for attempt in range(retries):
        elapsed = time.monotonic() - _last_request_at
        if elapsed < POLITE_DELAY_SECONDS:
            time.sleep(POLITE_DELAY_SECONDS - elapsed)
        _last_request_at = time.monotonic()

        request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})  # noqa: S310
        try:
            with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as response:  # noqa: S310
                return bytes(response.read())
        except (urllib.error.URLError, OSError, ValueError) as failure:
            last = failure
            time.sleep(2**attempt)
    raise HttpError(f"{url}: {last}")


def get_text(url: str, *, retries: int = 3) -> str:
    return get_bytes(url, retries=retries).decode("utf-8", errors="replace")


def get_json(url: str, *, retries: int = 3) -> Any:
    return json.loads(get_bytes(url, retries=retries))
