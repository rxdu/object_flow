"""Two backends' answers to the same calls, compared whole (flow-studio.md
§6, slice 4): a wire that sends every `/v1` and `/sandbox` call to both and
records where their answers differ, less the fields either names as not
computed, which a backend may leave out or fill in its own way.

A difference is named by where it is, never by the values, so a list of the
known ones stays the same from run to run: the module, the operation, and the
path into the answer, its list indexes dropped.
"""

from __future__ import annotations

import json
import re
import urllib.error
import urllib.request
from typing import Any, Iterator

from replay import Failed, NotRun, Wire

WIRE = ("/v1/", "/sandbox/")


def raw(base: str, method: str, path: str, body: Any) -> tuple[int, Any]:
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(base + path, data=data, method=method,
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req) as resp:
            return resp.status, json.loads(resp.read())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read() or b"{}")


def not_computed(*answers: Any) -> tuple[set, dict]:
    """The field names either answer lists as not computed, and the lists
    either keeps only the head of (`failures[1:]`); an entry in prose names
    no field and is left out."""
    names: set = set()
    heads: dict = {}
    for a in answers:
        if not isinstance(a, dict):
            continue
        for entry in a.get("not_computed") or []:
            if " " in entry:
                continue
            m = re.fullmatch(r"(\w+)\[(\d+):\]", entry)
            if m:
                heads[m.group(1)] = int(m.group(2))
            else:
                names.add(re.split(r"[.\[\]]+", entry.strip("[]."))[-1])
    return names, heads


def normalized(v: Any) -> Any:
    """An answer as the comparison reads it (ADR-0166): a change's `source`,
    `report` and `evidence`, which it holds as JSON text, read as the JSON
    objects they are, whose keys have no order;
    a publish report's findings compared as a set by code, without their
    messages, which are prose, or their samples, which `affected` holds
    whole; and the ids each category names, and the examples each
    transition is requested by, compared as sets."""
    if isinstance(v, list):
        return [normalized(x) for x in v]
    if not isinstance(v, dict):
        return v
    out = {}
    for k, x in v.items():
        if k in ("source", "report", "evidence") and isinstance(x, str) \
                and x.startswith("{"):
            try:
                x = json.loads(x)
            except ValueError:
                pass
        out[k] = normalized(x)
    findings = out.get("findings")
    if isinstance(findings, list) and findings and all(
            isinstance(f, dict) and "code" in f for f in findings):
        out["findings"] = sorted(
            ({k: x for k, x in f.items() if k not in ("message", "sample")}
             for f in findings), key=lambda f: f["code"])
        for k in ("affected", "examples"):
            if isinstance(out.get(k), dict):
                out[k] = {c: sorted(ids) if isinstance(ids, list) else ids
                          for c, ids in out[k].items()}
    return out


def pruned(v: Any, names: set, heads: dict) -> Any:
    if isinstance(v, dict):
        out = {}
        for k, x in v.items():
            if k in names or k == "not_computed":
                continue
            x = pruned(x, names, heads)
            if k in heads and isinstance(x, list):
                x = x[:heads[k]]
            out[k] = x
        return out
    if isinstance(v, list):
        return [pruned(x, names, heads) for x in v]
    return v


def differences(a: Any, b: Any, path: str = "") -> Iterator[tuple[str, str]]:
    """Where `a`, the core's, and `b`, the runner's, differ: each path and
    what differs there."""
    if isinstance(a, dict) and isinstance(b, dict):
        for k in sorted(set(a) | set(b)):
            if k not in a:
                yield f"{path}.{k}", "only the runner"
            elif k not in b:
                yield f"{path}.{k}", "only the core"
            else:
                yield from differences(a[k], b[k], f"{path}.{k}")
    elif isinstance(a, list) and isinstance(b, list):
        if len(a) != len(b):
            yield path, f"{len(a)} items in the core, {len(b)} in the runner"
        for i, (x, y) in enumerate(zip(a, b)):
            yield from differences(x, y, f"{path}[{i}]")
    elif a != b:
        yield path, f"core {json.dumps(a)[:80]}, runner {json.dumps(b)[:80]}"


class Tee(Wire):
    """The core's wire, every call of the engine's also sent to the runner
    and the two answers compared; the studio's tooling is the runner's
    server's. It answers as the core does."""

    def __init__(self, core: str, runner: str):
        super().__init__(core)
        self.core, self.runner = core, runner
        self.module = ""
        self.found: list[tuple[str, str, str, str, Any]] = []

    def call(self, method: str, path: str, body: Any = None) -> Any:
        if not path.startswith(WIRE):
            return raw(self.runner, method, path, body)[1]
        sc, ac = raw(self.core, method, path, body)
        sr, ar = raw(self.runner, method, path, body)
        if sc != sr:
            self.found.append((self.module, path, "", f"status: core {sc}, "
                               f"runner {sr}", body))
        elif sc == 200:
            names, heads = not_computed(ac, ar)
            for where, what in differences(pruned(normalized(ac), names, heads),
                                           pruned(normalized(ar), names, heads)):
                self.found.append((self.module, path, where, what, body))
        if sc >= 400:
            fault = ac.get("fault", "") if isinstance(ac, dict) else ""
            if sc == 501:
                raise NotRun(fault)
            raise Failed(f"the wire answered {sc}: {fault}")
        return ac

    @staticmethod
    def kind(found: tuple) -> str:
        """A difference by where it is: module, operation and path, the list
        indexes dropped."""
        m, p, w = found[:3]
        w = re.sub(r"\[\d+\]", "[]", w)
        return f"{m} {p} {w}".rstrip()

    def kinds(self) -> set[str]:
        return {self.kind(f) for f in self.found}
