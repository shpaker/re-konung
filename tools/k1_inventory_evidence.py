"""Dependency-free validation of inventory experiment traces, not game rules."""

from collections.abc import Iterable
from dataclasses import dataclass


@dataclass(frozen=True)
class RecalculationTrace:
    phase: str
    return_address: int
    actor: int
    health: int
    endurance: int
    equipment: tuple[int, ...]
    inventory: tuple[int, ...]
    cursor: int
    held: int
    screen: int


@dataclass(frozen=True)
class RecalculationEffect:
    before: RecalculationTrace
    after: RecalculationTrace

    @property
    def health_delta(self) -> int:
        return self.after.health - self.before.health

    @property
    def endurance_delta(self) -> int:
        return self.after.endurance - self.before.endurance

    @property
    def references_unchanged(self) -> bool:
        return (
            self.before.equipment == self.after.equipment
            and self.before.inventory == self.after.inventory
            and self.before.held == self.after.held
        )


def pair_recalculations(trace: Iterable[RecalculationTrace]) -> tuple[RecalculationEffect, ...]:
    """Pair observed calls in completion order; incomplete traces cannot prove a result."""
    pending = []
    effects = []
    for event in trace:
        if event.phase == "entry":
            pending.append(event)
        elif event.phase == "return":
            if not pending:
                raise ValueError("recalculation return has no observed entry")
            before = pending.pop()
            if (before.actor, before.return_address) != (event.actor, event.return_address):
                raise ValueError("recalculation return does not match the active call")
            effects.append(RecalculationEffect(before, event))
        else:
            raise ValueError(f"unknown recalculation phase: {event.phase}")
    if pending:
        raise ValueError("incomplete recalculation trace")
    return tuple(effects)
