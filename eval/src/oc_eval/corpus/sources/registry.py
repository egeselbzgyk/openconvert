"""The source names `oc-eval corpus harvest` accepts, and TEST_CORPUS §7.6's default target.

The default plan is §7.6's sourcing table, verbatim. `oapen-de` is the same adapter with a
language filter, and it is a separate line rather than a knob because §7.6 says a holdout that
reaches a hundred documents by dropping the German and Turkish slices has not met the target —
a target expressed as a plan the harvest executes is one the harvest cannot quietly miss.
"""

from __future__ import annotations

from collections.abc import Callable, Iterator
from typing import Any

from oc_eval.corpus.sources import Candidate, arxiv, dergipark, internet_archive, oapen, usgov

SourceFn = Callable[..., Iterator[Candidate]]


def _oapen_german(want: int, **kwargs: Any) -> Iterator[Candidate]:
    return oapen.candidates(want, language="German", **kwargs)


def _ia_german(want: int, **kwargs: Any) -> Iterator[Candidate]:
    return internet_archive.candidates(want, language="German", **kwargs)


SOURCES: dict[str, SourceFn] = {
    "oapen": oapen.candidates,
    "oapen-de": _oapen_german,
    "ia": internet_archive.candidates,
    "ia-de": _ia_german,
    "arxiv": arxiv.candidates,
    "usgov": usgov.candidates,
    "dergipark": dergipark.candidates,
}

# TEST_CORPUS §7.6: OAPEN ~40, IA PD scans ~20, arXiv ~15, US-Gov/EU ~15, DergiPark + DTA ~10.
# The OAPEN 40 is split so the German slice is sourced rather than hoped for, and asked for
# generously because a harvest rejects more candidates than it admits.
DEFAULT_PLAN = "oapen-de=20,oapen=25,ia=20,arxiv=15,usgov=15,dergipark=10"
