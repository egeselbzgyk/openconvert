"""A `llama-server` the gate starts and stops itself, the way the engine does (D8, PHASE 9
detail 2).

Loopback only, one slot, a per-run key in `LLAMA_API_KEY`, `--cache-reuse` only for an entry whose
registry says `cache_reuse = true`. The HTTP here is to `127.0.0.1` alone: `oc_eval.corpus.http`
stays the only place in `oc_eval` that opens a URL off the machine.
"""

from __future__ import annotations

import json
import os
import secrets
import socket
import subprocess
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

import psutil

from oc_eval import thresholds


@dataclass
class Completion:
    text: str
    reasoning: str | None
    cached_tokens: int | None
    seconds: float


class Server(Protocol):
    """What the gates need from a model server. `LlamaServer` is the real one; the tests fake it."""

    def start(self) -> bool: ...
    def complete(self, system: str, user: str, grammar: str) -> Completion: ...
    def tokenize_chat(self, system: str, user: str) -> list[int] | None: ...
    def rss_bytes(self) -> int: ...
    def stop(self) -> float | None: ...
    def bench(self) -> bool: ...


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


class LlamaServer:
    def __init__(
        self,
        program: Path,
        model: Path,
        *,
        context: int,
        cache_reuse: bool,
        context_checkpoints: int | None,
        bench: Path | None,
        threads: int,
    ) -> None:
        self.program = program
        self.model = model
        self.context = context
        self.cache_reuse = cache_reuse
        self.context_checkpoints = context_checkpoints
        self.bench_program = bench
        self.threads = threads
        self.key = secrets.token_hex(32)
        self.port = free_port()
        self.process: subprocess.Popen[bytes] | None = None
        self.peak_rss = 0

    def argv(self) -> list[str]:
        argv = [
            str(self.program),
            "--host",
            "127.0.0.1",
            "--port",
            str(self.port),
            "-np",
            "1",
            "-c",
            str(self.context),
            "-t",
            str(self.threads),
            "-m",
            str(self.model),
            "--chat-template-kwargs",
            '{"enable_thinking":false}',
        ]
        if self.cache_reuse:
            argv += ["--cache-reuse", str(thresholds.value("llm.cache_reuse_min_chunk"))]
        if self.context_checkpoints is not None:
            argv += ["--context-checkpoints", str(self.context_checkpoints)]
        return argv

    def start(self, timeout: float = 300.0) -> bool:
        if not self.program.is_file() or not self.model.is_file():
            return False
        env = dict(os.environ, LLAMA_API_KEY=self.key)
        self.process = subprocess.Popen(  # noqa: S603 - our own argv, no shell
            self.argv(), env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        )
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                return False
            try:
                if self._request("GET", "/health", None).get("status") == "ok":
                    return True
            except (urllib.error.URLError, OSError, ValueError):
                pass
            time.sleep(0.2)
        return False

    def _request(self, method: str, path: str, body: Any) -> Any:
        data = None if body is None else json.dumps(body).encode()
        request = urllib.request.Request(
            f"http://127.0.0.1:{self.port}{path}",
            data=data,
            method=method,
            headers={"Content-Type": "application/json", "Authorization": f"Bearer {self.key}"},
        )
        with urllib.request.urlopen(request, timeout=600) as response:  # noqa: S310
            return json.loads(response.read())

    def complete(self, system: str, user: str, grammar: str) -> Completion:
        started = time.monotonic()
        reply = self._request(
            "POST",
            "/v1/chat/completions",
            {
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": user},
                ],
                "temperature": thresholds.value("llm.temperature"),
                "max_tokens": thresholds.value("llm.max_output_tokens_per_call"),
                "grammar": grammar,
                "chat_template_kwargs": {"enable_thinking": False},
            },
        )
        self._sample_rss()
        message = reply["choices"][0]["message"]
        cached = (reply.get("timings") or {}).get("cache_n")
        if cached is None:
            cached = ((reply.get("usage") or {}).get("prompt_tokens_details") or {}).get(
                "cached_tokens"
            )
        return Completion(
            message.get("content") or "",
            message.get("reasoning_content") or None,
            cached,
            time.monotonic() - started,
        )

    def tokenize_chat(self, system: str, user: str) -> list[int] | None:
        templated = self._request(
            "POST",
            "/apply-template",
            {
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": user},
                ],
            },
        )["prompt"]
        tokens = self._request("POST", "/tokenize", {"content": templated})["tokens"]
        return [int(t) for t in tokens]

    def _sample_rss(self) -> None:
        self.peak_rss = max(self.peak_rss, self.rss_bytes())

    def rss_bytes(self) -> int:
        if self.process is None:
            return 0
        try:
            return int(psutil.Process(self.process.pid).memory_info().rss)
        except psutil.Error:
            return 0

    def stop(self) -> float | None:
        """Kill the server and return how long until its process was gone, or None if never."""
        if self.process is None:
            return None
        started = time.monotonic()
        self.process.kill()
        self.process.wait()
        pid = self.process.pid
        while psutil.pid_exists(pid) and time.monotonic() - started < 10:
            time.sleep(0.05)
        return None if psutil.pid_exists(pid) else time.monotonic() - started

    def bench(self) -> bool:
        if self.bench_program is None or not self.bench_program.is_file():
            return False
        done = subprocess.run(  # noqa: S603 - our own argv, no shell
            [str(self.bench_program), "-m", str(self.model), "-t", str(self.threads)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        return done.returncode == 0
