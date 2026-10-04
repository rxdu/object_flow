"""Values on the wire at their boundaries (ADR-0150 decision 2)."""

from fractions import Fraction

import pytest

from flowstudio import values
from flowstudio.scripts import flowrun


def test_absence_and_json_natives():
    assert values.encode(flowrun.ABSENT, ("text",)) is None
    assert values.encode(True, ("bool",)) is True
    assert values.encode("abc", ("text",)) == "abc"


def test_an_int_leaves_json_at_the_safe_range():
    assert values.encode(2**53 - 1, ("int",)) == 2**53 - 1
    assert values.encode(2**53, ("int",)) == str(2**53)
    assert values.encode(-(2**53), ("int",)) == str(-(2**53))


def test_a_decimal_keeps_its_scale_exactly():
    assert values.encode(Fraction("19.9"), ("decimal", 2)) == "19.90"
    assert values.encode(Fraction("-0.05"), ("decimal", 2)) == "-0.05"
    assert values.encode(Fraction(7), ("decimal", 0)) == "7"
    assert values.encode(Fraction("0.125"), ("decimal", None)) == "0.125"
    with pytest.raises(values.Unencodable):
        values.encode(Fraction("0.125"), ("decimal", 2))
    with pytest.raises(values.Unencodable):
        values.encode(Fraction(1, 3), ("decimal", None))


def test_money_carries_its_currency_and_minor_unit():
    assert values.encode(Fraction("12000"), ("money", "SGD")) == "SGD 12000.00"


def test_a_duration_is_written_in_its_largest_whole_unit():
    assert values.encode(Fraction(0), ("duration",)) == "0 s"
    assert values.encode(Fraction(90), ("duration",)) == "90 s"
    assert values.encode(Fraction(5400), ("duration",)) == "90 min"
    assert values.encode(Fraction(3600), ("duration",)) == "1 h"
    assert values.encode(Fraction(86400), ("duration",)) == "1 day"
    assert values.encode(Fraction(2 * 86400), ("duration",)) == "2 days"
    assert values.encode(Fraction(604800), ("duration",)) == "1 week"
    assert values.encode(Fraction(-3600), ("duration",)) == "-1 h"
    assert flowrun.seconds("90 min") == 5400


def test_a_timestamp_is_rfc_3339_at_utc_and_reads_back():
    t = flowrun.timestamp("2026-10-04T09:30:00Z")
    assert values.encode(t, ("timestamp",)) == "2026-10-04T09:30:00Z"
    with pytest.raises(values.Unencodable):
        values.encode(t + Fraction(1, 2), ("timestamp",))


def test_states_members_and_sets_are_written_by_name():
    assert values.encode(("state", "Job", "OPEN"), ("unknown",)) == "OPEN"
    assert values.encode(("enum", "Priority", "HIGH"), ("enum", "Priority")) \
        == "HIGH"
    assert values.encode(("a-1", "a-2"), ("set", ("ref",))) == ["a-1", "a-2"]


def test_an_untyped_fraction_is_refused_not_guessed():
    with pytest.raises(values.Unencodable):
        values.encode(Fraction(1, 2), ("unknown",))


def test_a_member_on_the_wire_is_qualified_for_the_runner():
    enums = {"Priority": ["LOW", "HIGH"]}
    spec = {"type": "Priority"}
    assert values.decode_input("HIGH", spec, enums) == "Priority.HIGH"
    assert values.decode_input("Priority.HIGH", spec, enums) \
        == "Priority.HIGH"
    assert values.decode_input(["LOW"], {"type": "Priority[]"}, enums) \
        == ["Priority.LOW"]
