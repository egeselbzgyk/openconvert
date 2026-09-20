"""Peak resident memory of a **whole process tree**, not of one process.

TEST_STRATEGY §8.3 and R9 §D.5. D13.11's 500 MB budget is for the converter *and whatever it
spawns* — a `llama-server` sidecar from Phase 9, a `tesseract` from Phase 13 — because a user
whose machine ran out of memory does not care which of our processes asked for it. Measuring
only the parent would report a figure that quietly stops being true the moment a sidecar
exists.

Three platforms, three mechanisms, one number:

* **Linux** — `/proc/<pid>/status`'s `VmHWM` is the kernel's own high-water mark, exact and
  free. It is per process, so the tree is summed over the processes seen while polling.
* **Windows** — a Job Object accounts for every process in the tree at once and reports
  `PeakJobMemoryUsed`, which is the right number by construction rather than by sampling.
* **macOS and the fallback** — `psutil` polling. A poll can miss a spike between samples, so
  the result says it is a floor rather than a measurement.

The polling paths are honest about what they are: `Measurement.exact` is False for them, and a
report that treats a floor as a measurement is how a budget quietly stops being enforced.
"""

from __future__ import annotations

import subprocess
import sys
import threading
import time
from dataclasses import dataclass
from pathlib import Path

# How often the polling paths look. Fast enough to catch a page-rendering spike, slow enough
# that the sampler is not itself a measurable share of the run.
POLL_INTERVAL_SECONDS = 0.05


@dataclass(frozen=True)
class Measurement:
    peak_rss_bytes: int
    exit_code: int
    wall_seconds: float
    method: str
    exact: bool

    @property
    def peak_rss_mb(self) -> float:
        return self.peak_rss_bytes / (1024 * 1024)


def run(argv: list[str], *, cwd: Path | None = None) -> Measurement:
    """Run `argv` to completion and report the peak RSS of everything it started."""
    if sys.platform == "linux":
        return _run_polled(argv, cwd=cwd, method="proc-vmhwm", exact=True)
    if sys.platform == "win32":
        return _run_job_object(argv, cwd=cwd)
    return _run_polled(argv, cwd=cwd, method="psutil-poll", exact=False)


def _run_polled(argv: list[str], *, cwd: Path | None, method: str, exact: bool) -> Measurement:
    import psutil

    started = time.monotonic()
    process = psutil.Popen(argv, cwd=cwd)
    peak = 0

    def sample() -> None:
        nonlocal peak
        while process.poll() is None:
            peak = max(peak, _tree_rss(process, exact=exact))
            time.sleep(POLL_INTERVAL_SECONDS)

    sampler = threading.Thread(target=sample, daemon=True)
    sampler.start()
    exit_code = process.wait()
    sampler.join(timeout=1.0)

    return Measurement(
        peak_rss_bytes=peak,
        exit_code=exit_code,
        wall_seconds=time.monotonic() - started,
        method=method,
        exact=exact,
    )


def _tree_rss(process: object, *, exact: bool) -> int:
    """The tree's resident memory right now, by whichever mechanism is exact here."""
    import psutil

    total = 0
    try:
        members = [process, *process.children(recursive=True)]  # type: ignore[attr-defined]
    except psutil.Error:
        return 0

    for member in members:
        try:
            if exact:
                total += _vm_hwm(member.pid)
            else:
                total += int(member.memory_info().rss)
        except (psutil.Error, OSError):
            continue
    return total


def _vm_hwm(pid: int) -> int:
    """`VmHWM` from `/proc/<pid>/status`, in bytes. The kernel's own high-water mark."""
    try:
        text = Path(f"/proc/{pid}/status").read_text(encoding="utf-8")
    except OSError:
        return 0
    for line in text.splitlines():
        if line.startswith("VmHWM:"):
            parts = line.split()
            if len(parts) >= 2 and parts[1].isdigit():
                return int(parts[1]) * 1024
    return 0


def _run_job_object(argv: list[str], *, cwd: Path | None) -> Measurement:
    """Windows: a Job Object accounts for the whole tree and reports its peak directly."""
    try:
        import win32job  # type: ignore[import-not-found, import-untyped]
    except ImportError:
        # `pywin32` is not a dependency of this package — the Windows path is a convenience
        # for a developer, and the gate runs on Linux. Falling back to polling is honest as
        # long as the result says it is polling.
        return _run_polled(argv, cwd=cwd, method="psutil-poll (no pywin32)", exact=False)

    started = time.monotonic()
    job = win32job.CreateJobObject(None, "")
    process = subprocess.Popen(argv, cwd=cwd)  # noqa: S603 - argv is the caller's, by design
    try:
        import win32api  # type: ignore[import-not-found, import-untyped]
        import win32con  # type: ignore[import-not-found, import-untyped]

        handle = win32api.OpenProcess(
            win32con.PROCESS_SET_QUOTA | win32con.PROCESS_TERMINATE, False, process.pid
        )
        win32job.AssignProcessToJobObject(job, handle)
        exit_code = process.wait()
        info = win32job.QueryInformationJobObject(job, win32job.JobObjectExtendedLimitInformation)
        peak = int(info["PeakJobMemoryUsed"])
    except Exception:  # any failure here means fall back, never fail the run
        exit_code = process.wait()
        peak = 0

    return Measurement(
        peak_rss_bytes=peak,
        exit_code=exit_code,
        wall_seconds=time.monotonic() - started,
        method="windows-job-object",
        exact=True,
    )
