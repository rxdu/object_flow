"""Values on the wire (ADR-0150 decision 2).

JSON's own where JSON keeps a value exactly: absence is null, a bool a
boolean, an int a number within the safe range, text and an identity a
string, a reference the object's id, a set an array. Every other value is a
string in the model's notation (`declaration-syntax.md` §8.1): a decimal
"19.99", money "USD 19.99", a duration "30 min", a date "2026-10-04", a
timestamp in RFC 3339 at UTC, a state or an enumeration member by its name.
A reader knows each value's type from the declaration.
"""

from __future__ import annotations

import datetime
import re
from fractions import Fraction
from typing import Any

from .scripts import flowrun

SAFE_INT = 2**53 - 1
EPOCH = datetime.datetime(1970, 1, 1, tzinfo=datetime.timezone.utc)
UNITS = (("week", 604800), ("day", 86400), ("h", 3600), ("min", 60), ("s", 1))
PLURAL = {"week": "weeks", "day": "days"}


class Unencodable(Exception):
    """A value the wire cannot carry without guessing its type."""


def type_of_spec(spec: dict | None, enums: dict) -> tuple:
    """An attribute's or input's declared type, as the encoder reads it."""
    spec = spec or {}
    if "reference" in spec:
        ref = spec["reference"]
        return ("set", ("ref",)) if ref.endswith("[]") else ("ref",)
    typ = str(spec.get("type", ""))
    if typ.endswith("[]"):
        return ("set", type_of_spec(dict(spec, type=typ[:-2]), enums))
    m = re.fullmatch(r"decimal\((\d+),\s*(\d+)\)", typ)
    if m:
        return ("decimal", int(m.group(2)))
    m = re.fullmatch(r"money\((\w+)\)", typ)
    if m:
        return ("money", m.group(1))
    if typ in ("int", "counter"):
        return ("int",)
    if typ in ("timestamp", "date", "duration", "bool"):
        return (typ,)
    if typ in enums:
        return ("enum", typ)
    if typ:
        return ("text",)
    return ("unknown",)


def type_of_kind(kind: tuple) -> tuple:
    """An expression's type, as the runner infers it, as the encoder reads it."""
    k = kind[0]
    if k in ("decimal", "literal"):
        return ("decimal", kind[1])
    if k == "money":
        return ("money", kind[1])
    if k in ("int", "duration", "timestamp", "bool"):
        return (k,)
    if k == "object":
        return ("ref",)
    if k == "objects":
        return ("set", ("ref",))
    return ("unknown",)


def decimal_text(v: Fraction, scale: int | None) -> str:
    """A decimal written exactly, to its declared scale where it has one."""
    places = scale
    if places is None:
        places = 0
        while (v * 10**places).denominator != 1:
            places += 1
            if places > 38:
                raise Unencodable(f"{v} has no exact decimal form")
    scaled = v * 10**places
    if scaled.denominator != 1:
        raise Unencodable(f"{v} has more places than its scale, {places}")
    n = int(scaled)
    sign, digits = ("-" if n < 0 else ""), str(abs(n)).rjust(places + 1, "0")
    if not places:
        return sign + digits
    return f"{sign}{digits[:-places]}.{digits[-places:]}"


def timestamp_text(v: Fraction) -> str:
    if Fraction(v).denominator != 1:
        raise Unencodable(f"a timestamp of {v} seconds is not whole")
    t = EPOCH + datetime.timedelta(seconds=int(v))
    return t.strftime("%Y-%m-%dT%H:%M:%SZ")


def duration_text(v: Fraction) -> str:
    if Fraction(v).denominator != 1:
        raise Unencodable(f"a duration of {v} seconds is not whole")
    n = int(v)
    for unit, size in UNITS:
        if n % size == 0 and (n or unit == "s"):
            count = n // size
            name = PLURAL.get(unit, unit) if abs(count) != 1 else unit
            return f"{count} {name}"
    raise AssertionError("unreachable: every whole duration divides by 1 s")


def encode(v: Any, typ: tuple) -> Any:
    """A runner's value as the wire carries it, given its declared type."""
    if v is flowrun.ABSENT or v is None:
        return None
    if isinstance(v, bool):
        return v
    if isinstance(v, tuple) and v and v[0] == "state":
        return v[2]
    if isinstance(v, tuple) and v and v[0] == "enum":
        return v[2]
    if isinstance(v, tuple):
        inner = typ[1] if typ[0] == "set" else ("unknown",)
        return [encode(x, inner) for x in v]
    if isinstance(v, datetime.date):
        return v.isoformat()
    if isinstance(v, str):
        return v
    if isinstance(v, (int, Fraction)):
        k = typ[0]
        if k == "timestamp":
            return timestamp_text(Fraction(v))
        if k == "duration":
            return duration_text(Fraction(v))
        if k == "decimal":
            return decimal_text(Fraction(v), typ[1])
        if k == "money":
            minor = flowrun.MINOR_UNITS.get(typ[1])
            return f"{typ[1]} {decimal_text(Fraction(v), minor)}"
        if Fraction(v).denominator == 1 and k in ("int", "unknown", "text"):
            n = int(v)
            return n if abs(n) <= SAFE_INT else str(n)
        raise Unencodable(f"{v} of type {typ}")
    raise Unencodable(f"{v!r} of type {typ}")


def decode_input(v: Any, spec: dict | None, enums: dict) -> Any:
    """A wire value as the runner reads an input written in the examples
    format (`flow-format.md` §11.3): the same notation, except that an
    enumeration member is written by its name on the wire and qualified in an
    example."""
    typ = type_of_spec(spec, enums)
    if v is None:
        return None
    if typ[0] == "set" and isinstance(v, list):
        inner = dict(spec or {})
        if "type" in inner:
            inner["type"] = inner["type"][:-2]
        return [decode_input(x, inner, enums) for x in v]
    if typ[0] == "enum" and isinstance(v, str) and "." not in v:
        return f"{typ[1]}.{v}"
    return v
