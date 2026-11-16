"""Regression cases for selection policy independently exercised on both EXEs."""

import pytest

from tools.audit_k1_equipment_render import SelectionPolicy, selection_policy


@pytest.mark.parametrize(
    "inputs,expected",
    [
        ((5, 7, 0, 1), SelectionPolicy("unarmed", 7, ())),
        ((1, 3, 4, 1), SelectionPolicy("bow", 27, (1,))),
        ((4, 6, 4, 1), SelectionPolicy("bow", 14, (1,))),
        ((5, 2, 4, 1), SelectionPolicy("melee", 18, (5,))),
        ((5, 2, 4, 2), SelectionPolicy("melee", 18, (2, 5))),
        ((0, 0, 4, 0), SelectionPolicy("melee", 0, (0, 5))),
        ((7, 5, 4, 1), SelectionPolicy("bow", 5, (1,))),
        ((6, 1, 4, 2), SelectionPolicy("melee", 1, (2, 5))),
    ],
)
def test_original_slot_selection_distinguishes_bow_melee_and_raw_state(inputs, expected):
    assert selection_policy(*inputs) == expected
