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


def test_a_value_is_written_as_its_type_holds_it():
    """The runner writes a value as the core's `fit_value` does: an integer
    to a decimal at its scale, a number to an integer only where it is
    whole, and refuses one of another type `value_in_range`, naming the
    object (the audit's values W1 to W3, ADR-0205 decision 1)."""
    fit = flowrun.at_declared_scale
    assert fit({"type": "decimal(10,2)"}, 5) == Fraction(5)
    assert fit({"type": "decimal(12,4)"}, Fraction(10, 3)) == Fraction("3.3333")
    assert fit({"type": "int"}, Fraction(21)) == 21
    for spec, v in (({"type": "int"}, Fraction(21, 2)),
                    ({"type": "int"}, "3"),
                    ({"type": "decimal(10,2)"}, "5"),
                    ({"type": "string"}, 3),
                    ({"type": "bool"}, 1),
                    ({"type": "Priority"}, ("enum", "Level", "HIGH"))):
        with pytest.raises(flowrun.Refused) as refused:
            fit(spec, v, "item-1")
        assert (refused.value.clause, refused.value.objects) == \
            ("value_in_range", ["item-1"]), (spec, v)


def test_what_the_engine_cannot_hold_is_not_run():
    """A literal the engine cannot hold exactly, a fraction of a second or an
    amount past its currency's minor unit or in a currency it does not know,
    and `time_in` of what is no state, are not run by the runner as by the
    core, never rounded or answered 0 s (the audit's values L1, L2 and
    G2)."""
    from types import SimpleNamespace
    store = SimpleNamespace(kinds={"T": {"states": {"OPEN": {}}}}, now=0)
    ev = flowrun.Evaluator(store, "T")
    for text in ("1.5 s", "USD 1.005", "JPY 2.5", "PHP 1.00"):
        with pytest.raises(flowrun.NotRunnable):
            ev.value(flowrun.parsed(text), {})
    assert ev.value(flowrun.parsed("0.5 min"), {}) == 30
    assert ev.value(flowrun.parsed("USD 1.50"), {}) == Fraction(3, 2)
    o = SimpleNamespace(type="T", spans={})
    with pytest.raises(flowrun.NotRunnable):
        ev.time_in(o, "label")
    assert ev.time_in(o, "OPEN") == 0
