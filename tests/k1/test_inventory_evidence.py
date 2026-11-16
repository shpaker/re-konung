"""Synthetic trace contracts run without originals or emulator dependencies."""

from dataclasses import replace

import pytest

from tools.k1_inventory_evidence import RecalculationTrace, pair_recalculations


def entry(**changes):
    return replace(
        RecalculationTrace("entry", 100, 200, 100, 23, (0, 0, 1, 0, 0, 0), (2, 3), 4, 0, 0),
        **changes,
    )


def test_same_item_references_do_not_hide_health_or_endurance_mutations():
    before = entry()
    after = replace(before, phase="return", health=132, endurance=20)
    (effect,) = pair_recalculations((before, after))
    assert effect.references_unchanged
    assert effect.health_delta == 32
    assert effect.endurance_delta == -3


def test_repeated_redraws_are_separate_observations():
    first = entry()
    second = entry(health=116)
    effects = pair_recalculations(
        (
            first,
            replace(first, phase="return", health=116),
            second,
            replace(second, phase="return", health=132),
        )
    )
    assert len(effects) == 2
    assert [effect.health_delta for effect in effects] == [16, 16]


@pytest.mark.parametrize(
    "changes", [{"inventory": (3, 2)}, {"equipment": (1, 0, 0, 0, 0, 0)}, {"held": 1}]
)
def test_reordered_or_transferred_references_are_visible(changes):
    before = entry()
    (effect,) = pair_recalculations((before, replace(before, phase="return", **changes)))
    assert not effect.references_unchanged


def test_nested_calls_are_paired_by_actor_and_return_address():
    outer = entry()
    inner = entry(actor=201, return_address=101)
    effects = pair_recalculations(
        (outer, inner, replace(inner, phase="return"), replace(outer, phase="return"))
    )
    assert [effect.before.actor for effect in effects] == [201, 200]


@pytest.mark.parametrize(
    "trace",
    [
        (entry(),),
        (entry(phase="return"),),
        (entry(), entry(phase="return", actor=201)),
        (entry(), entry(phase="return", return_address=101)),
        (entry(phase="unknown"),),
    ],
)
def test_incomplete_or_mismatched_traces_cannot_pass(trace):
    with pytest.raises(ValueError):
        pair_recalculations(trace)
