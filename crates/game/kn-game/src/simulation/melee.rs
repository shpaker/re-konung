use super::*;
use crate::movement::grid::Direction;
use k1_game::combat::{self, Combatant};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackError {
    AttackerUnavailable,
    TargetUnavailable,
    InvalidTarget,
    MissingAnimation,
    NoRoute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackStatus {
    Pending,
    Approaching,
    Attacking,
    Finished,
    Refused(AttackError),
}

impl AttackStatus {
    fn active(self) -> bool {
        matches!(self, Self::Pending | Self::Approaching | Self::Attacking)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackOrder {
    pub target_id: usize,
    pub status: AttackStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PursuitError {
    TargetUnavailable,
    NoRoute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PursuitStatus {
    Pending,
    Approaching,
    InRange,
    Finished,
    Refused(PursuitError),
}

impl PursuitStatus {
    fn active(self) -> bool {
        matches!(self, Self::Pending | Self::Approaching | Self::InRange)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DebugPursuit {
    pub target_id: u64,
    /// Last assigned target cell, retained after completion for inspection.
    pub target_cell: (u32, u32),
    pub status: PursuitStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CombatTarget {
    Character(usize),
    DebugPlayer(u64),
}

#[derive(Debug, Clone, Copy)]
enum HeroCommand {
    Move(Cell, MovementMode),
    Attack(usize),
}

pub(super) struct CombatRuntime {
    frames: Vec<[u32; DIRECTION_COUNT]>,
    orders: Vec<Option<AttackOrder>>,
    pub(super) pursuits: Vec<Option<DebugPursuit>>,
    target_cells: Vec<Option<Cell>>,
    retry_at: Vec<u64>,
    retreating: Vec<bool>,
    leashes: Vec<game_state::Rect>,
    queued_hero: Option<HeroCommand>,
    hero_feedback: Option<AttackOrder>,
    hero_manual_attack: bool,
}

impl CombatRuntime {
    pub(super) fn new(groups: &[Option<game_state::Group>], roster: &[Creature]) -> Self {
        Self {
            frames: vec![[0; DIRECTION_COUNT]; roster.len()],
            orders: vec![None; roster.len()],
            pursuits: vec![None; roster.len()],
            target_cells: vec![None; roster.len()],
            retry_at: vec![0; roster.len()],
            retreating: vec![false; roster.len()],
            leashes: roster
                .iter()
                .map(|creature| {
                    groups
                        .get(creature.group)
                        .and_then(Option::as_ref)
                        .map_or(creature.spawn, |group| group.leash)
                })
                .collect(),
            queued_hero: None,
            hero_feedback: None,
            hero_manual_attack: false,
        }
    }
}

impl Simulation {
    /// Counts come from the loaded sprite tables; zero means unsupported, never a guessed duration.
    pub fn configure_melee_frames(&mut self, counts: Vec<[u32; DIRECTION_COUNT]>) {
        self.melee.frames.fill([0; DIRECTION_COUNT]);
        for (slot, counts) in self.melee.frames.iter_mut().zip(counts) {
            *slot = counts;
        }
    }

    pub fn attack_order(&self, creature_id: usize) -> Option<AttackOrder> {
        let index = self
            .roster
            .iter()
            .position(|creature| creature.id == creature_id)?;
        self.melee.orders[index]
    }

    pub fn debug_pursuit(&self, creature_id: usize) -> Option<DebugPursuit> {
        self.record_index(creature_id)
            .and_then(|index| self.melee.pursuits[index])
    }

    pub fn hero_attack_order(&self) -> Option<AttackOrder> {
        self.melee.hero_feedback
    }

    fn combatant(&self, index: usize) -> Option<Combatant> {
        let creature = &self.roster[index];
        let state = self.world.state(self.ids[index]?)?;
        Some(Combatant {
            id: creature.id,
            group: creature.group,
            group_kind: creature.group_kind,
            class_id: creature.class_id,
            state: creature.state,
            cell: (state.cell.column as i16, state.cell.row as i16),
            target: self.melee.orders[index]
                .filter(|order| order.status.active())
                .map(|order| order.target_id),
        })
    }

    fn targeting_combatant(&self, index: usize) -> Option<Combatant<CombatTarget>> {
        let actor = self.combatant(index)?;
        Some(Combatant {
            id: CombatTarget::Character(actor.id),
            group: actor.group,
            group_kind: actor.group_kind,
            class_id: actor.class_id,
            state: actor.state,
            cell: actor.cell,
            target: self.melee.pursuits[index]
                .filter(|pursuit| pursuit.status.active())
                .map(|pursuit| CombatTarget::DebugPlayer(pursuit.target_id))
                .or_else(|| actor.target.map(CombatTarget::Character)),
        })
    }

    fn debug_combatant(&self) -> Option<Combatant<CombatTarget>> {
        let actor = self.debug_actor.as_ref()?;
        let state = self.world.state(actor.world_id)?;
        Some(Combatant {
            id: CombatTarget::DebugPlayer(actor.id),
            group: actor.profile.group,
            group_kind: k1_game::rules::group::PLAYER,
            class_id: actor.profile.class_id,
            state: actor.profile.recorded_state,
            cell: (state.cell.column as i16, state.cell.row as i16),
            target: None,
        })
    }

    fn record_index(&self, id: usize) -> Option<usize> {
        self.roster.iter().position(|creature| creature.id == id)
    }

    pub fn can_hero_attack(&self, target_id: usize) -> bool {
        self.hero_id
            .and_then(|id| self.record_index(id))
            .and_then(|index| self.combatant(index))
            .zip(
                self.record_index(target_id)
                    .and_then(|index| self.combatant(index)),
            )
            .is_some_and(|(attacker, target)| combat::opponents(attacker, target))
    }

    fn validate_attack(&self, index: usize, target_id: usize) -> Result<(), AttackError> {
        let attacker = self
            .combatant(index)
            .filter(|actor| actor.alive())
            .ok_or(AttackError::AttackerUnavailable)?;
        let target = self
            .record_index(target_id)
            .and_then(|index| self.combatant(index))
            .filter(|actor| actor.alive())
            .ok_or(AttackError::TargetUnavailable)?;
        if !combat::opponents(attacker, target) {
            return Err(AttackError::InvalidTarget);
        }
        if self.melee.frames[index].iter().all(|frames| *frames == 0) {
            return Err(AttackError::MissingAnimation);
        }
        Ok(())
    }

    pub fn order_hero_attack(&mut self, target_id: usize) -> Result<usize, AttackError> {
        let result = self
            .hero_id
            .and_then(|id| self.record_index(id))
            .ok_or(AttackError::AttackerUnavailable)
            .and_then(|index| self.validate_attack(index, target_id).map(|()| index));
        let index = match result {
            Ok(index) => index,
            Err(error) => {
                self.melee.hero_feedback = Some(AttackOrder {
                    target_id,
                    status: AttackStatus::Refused(error),
                });
                return Err(error);
            }
        };
        self.hero_order = None;
        self.hero_active_target = None;
        self.melee.hero_manual_attack = true;
        self.melee.hero_feedback = Some(AttackOrder {
            target_id,
            status: AttackStatus::Pending,
        });
        if matches!(
            self.creature_stage(self.roster[index].id),
            Some(CreatureStage::Melee { .. })
        ) {
            self.melee.queued_hero = Some(HeroCommand::Attack(target_id));
        } else {
            self.melee.queued_hero = None;
            self.begin_attack(index, target_id);
        }
        Ok(0)
    }

    fn begin_attack(&mut self, index: usize, target_id: usize) {
        self.melee.pursuits[index] = None;
        if self.melee.orders[index]
            .is_some_and(|order| order.target_id == target_id && order.status.active())
        {
            return;
        }
        if self.melee.orders[index].is_none_or(|order| order.target_id != target_id) {
            self.melee.retry_at[index] = 0;
        }
        if let Some(id) = self.ids[index] {
            // Finish the current step, but discard any previous destination.
            let _ = self.world.stop(id);
        }
        self.melee.orders[index] = Some(AttackOrder {
            target_id,
            status: AttackStatus::Pending,
        });
        self.melee.target_cells[index] = None;
        self.melee.retreating[index] = false;
    }

    pub(super) fn defer_combat_move(
        &mut self,
        target: Cell,
        mode: MovementMode,
    ) -> Result<bool, HeroOrderError> {
        if !self.world.grid().contains(target) {
            return Ok(false);
        }
        let Some(id) = self.hero_id else {
            return Ok(false);
        };
        if !matches!(self.creature_stage(id), Some(CreatureStage::Melee { .. })) {
            return Ok(false);
        }
        self.melee.queued_hero = Some(HeroCommand::Move(target, mode));
        self.melee.hero_feedback = None;
        self.melee.hero_manual_attack = false;
        self.hero_active_target = None;
        self.hero_order = Some(HeroOrder {
            target,
            mode,
            status: HeroOrderStatus::Pending,
        });
        Ok(true)
    }

    pub(super) fn clear_hero_attack(&mut self) {
        if let Some(index) = self.hero_id.and_then(|id| self.record_index(id)) {
            self.melee.orders[index] = None;
            self.melee.target_cells[index] = None;
        }
        self.melee.queued_hero = None;
        self.melee.hero_feedback = None;
        self.melee.hero_manual_attack = false;
    }

    pub(super) fn reset_combat(&mut self) {
        for creature in &self.roster {
            self.creature_anim.cancel_melee(creature.id);
        }
        self.melee.orders.fill(None);
        self.melee.pursuits.fill(None);
        self.melee.target_cells.fill(None);
        self.melee.retry_at.fill(0);
        self.melee.retreating.fill(false);
        self.clear_hero_attack();
    }

    fn set_attack_status(&mut self, index: usize, status: AttackStatus) {
        if let Some(order) = self.melee.orders[index].as_mut() {
            order.status = status;
            if Some(self.roster[index].id) == self.hero_id && self.melee.queued_hero.is_none() {
                self.melee.hero_feedback = Some(*order);
            }
        }
    }

    fn finish_attack(&mut self, index: usize, status: AttackStatus) {
        if Some(self.roster[index].id) == self.hero_id {
            self.melee.hero_manual_attack = false;
        }
        self.creature_anim.cancel_melee(self.roster[index].id);
        if let Some(id) = self.ids[index] {
            let _ = self.world.stop(id);
        }
        self.set_attack_status(index, status);
        self.melee.target_cells[index] = None;
        if matches!(status, AttackStatus::Refused(_)) {
            self.melee.retry_at[index] =
                self.tick + u64::from(self.world.rules().repath_cooldown_ticks);
        }
    }

    /// The original marks the victim's squad on frame two, independently of damage.
    fn provoke_squad(&mut self, target_id: usize) {
        let Some(target) = self.record_index(target_id) else {
            return;
        };
        let group = self.roster[target].group;
        for index in 0..self.roster.len() {
            if self.roster[index].group != group || !self.roster[index].roaming {
                continue;
            }
            self.group_states[index] = GroupState::PlayerInside;
            if !self.melee.orders[index].is_some_and(|order| order.status.active())
                && self.roster[index].class_id != 0x49
                && let Some(id) = self.ids[index]
            {
                let _ = self.world.stop(id);
            }
        }
    }

    pub(super) fn tick_combat(&mut self) -> Vec<bool> {
        let mut provoked_group = None;
        if let Some(hero) = self.hero_id
            && !matches!(self.creature_stage(hero), Some(CreatureStage::Melee { .. }))
            && let Some(command) = self.melee.queued_hero.take()
        {
            match command {
                HeroCommand::Move(target, mode) => {
                    let _ = self.order_hero_move_with_mode(target, mode);
                }
                HeroCommand::Attack(target) => {
                    let _ = self.order_hero_attack(target);
                }
            }
        }
        if let Some(hero) = self.hero_id
            && matches!(
                self.creature_stage(hero),
                Some(CreatureStage::Melee { frame: 2 })
            )
            && let Some(order) = self.attack_order(hero)
            && let Some(index) = self.record_index(hero)
            && self.validate_attack(index, order.target_id).is_ok()
            && let Some(target_index) = self.record_index(order.target_id)
            && let Some(target) = self.world.state(self.ids[target_index].unwrap())
            && self.melee.target_cells[index] == Some(target.cell)
        {
            provoked_group = Some(self.roster[target_index].group);
            self.provoke_squad(order.target_id);
        }

        let mut snapshot: Vec<_> = (0..self.roster.len())
            .filter_map(|index| self.targeting_combatant(index))
            .collect();
        snapshot.extend(self.debug_combatant());
        let mut busy = vec![false; self.roster.len()];
        for (index, handled) in busy.iter_mut().enumerate() {
            let Some(actor) = self.combatant(index) else {
                continue;
            };
            if !actor.alive() {
                continue;
            }
            let id = self.ids[index].unwrap();
            let state = self.world.state(id).unwrap();
            if matches!(
                self.creature_stage(actor.id),
                Some(CreatureStage::Special { .. })
            ) {
                *handled = true;
                continue;
            }
            if self.melee.retreating[index] {
                if state.moving || state.goal.is_some() {
                    *handled = true;
                    continue;
                }
                self.melee.retreating[index] = false;
            }
            let active = self.melee.orders[index].is_some_and(|order| order.status.active())
                || self.melee.pursuits[index].is_some_and(|pursuit| pursuit.status.active());
            if !active && self.tick >= self.melee.retry_at[index] {
                let explicit_move = Some(actor.id) == self.hero_id
                    && self.hero_order.is_some_and(|order| {
                        matches!(
                            order.status,
                            HeroOrderStatus::Pending | HeroOrderStatus::Moving
                        )
                    });
                let target = if !explicit_move && !state.moving && state.goal.is_none() {
                    let actor = self.targeting_combatant(index).unwrap();
                    combat::retaliation(actor, &snapshot).or_else(|| {
                        (self.group_states[index] == GroupState::PlayerInside
                            || provoked_group == Some(actor.group))
                        .then(|| combat::monster_target(actor, &snapshot))
                        .flatten()
                    })
                } else {
                    None
                };
                if let Some(target) = target {
                    match target {
                        CombatTarget::Character(target_id) => {
                            self.begin_attack(index, target_id);
                            if let Err(error) = self.validate_attack(index, target_id) {
                                self.finish_attack(index, AttackStatus::Refused(error));
                                continue;
                            }
                        }
                        CombatTarget::DebugPlayer(target_id) => {
                            self.begin_debug_pursuit(index, target_id);
                        }
                    }
                    if let Some(entry) = snapshot
                        .iter_mut()
                        .find(|entry| entry.id == CombatTarget::Character(actor.id))
                    {
                        entry.target = Some(target);
                    }
                }
            }
            if self.melee.pursuits[index].is_some_and(|pursuit| pursuit.status.active()) {
                *handled = self.tick_debug_pursuit(index);
                continue;
            }
            let Some(order) = self.melee.orders[index].filter(|order| order.status.active()) else {
                continue;
            };
            *handled = true;
            if let Err(error) = self.validate_attack(index, order.target_id) {
                self.finish_attack(index, AttackStatus::Refused(error));
                continue;
            }
            let target_index = self.record_index(order.target_id).unwrap();
            let target = self
                .world
                .state(self.ids[target_index].unwrap())
                .unwrap()
                .cell;
            let stage = self.creature_stage(actor.id);
            if matches!(stage, Some(CreatureStage::Melee { .. })) {
                if self.melee.target_cells[index] != Some(target) {
                    if Some(actor.id) == self.hero_id && self.melee.hero_manual_attack {
                        self.creature_anim.cancel_melee(actor.id);
                        self.melee.target_cells[index] = None;
                        self.set_attack_status(index, AttackStatus::Pending);
                    } else {
                        self.finish_attack(index, AttackStatus::Finished);
                    }
                }
                continue;
            }
            if order.status == AttackStatus::Attacking
                && actor.class_id == 0x49
                && self.start_retreat(index)
            {
                continue;
            }
            if actor.roaming()
                && self.melee.target_cells[index].is_some_and(|previous| previous != target)
                && (state.moving || state.goal.is_some())
                && !inside_rectangle(self.melee.leashes[index], target)
            {
                self.finish_attack(index, AttackStatus::Finished);
                continue;
            }
            if state.moving {
                if self.melee.target_cells[index] != Some(target) {
                    self.approach_target(index, target);
                }
                continue;
            }
            if let Some(direction) = Direction::between(state.cell, target) {
                let frames = self.melee.frames[index][direction.index()];
                if frames == 0 {
                    self.finish_attack(index, AttackStatus::Refused(AttackError::MissingAnimation));
                    continue;
                }
                let _ = self.world.stop(id);
                let _ = self.world.turn(id, direction);
                self.creature_anim.arm(actor.id);
                if self.creature_anim.start_melee(actor.id, frames) {
                    self.melee.target_cells[index] = Some(target);
                    self.set_attack_status(index, AttackStatus::Attacking);
                }
            } else if (combat::ambush(actor.class_id) && actor.roaming())
                || (Some(actor.id) == self.hero_id && !self.melee.hero_manual_attack)
            {
                self.finish_attack(index, AttackStatus::Finished);
            } else if self.tick >= self.melee.retry_at[index] {
                self.approach_target(index, target);
            }
        }
        busy
    }

    fn begin_debug_pursuit(&mut self, index: usize, target_id: u64) {
        let Some(target) = self
            .debug_actor
            .as_ref()
            .filter(|actor| actor.id == target_id)
            .and_then(|actor| self.world.state(actor.world_id))
            .map(|state| state.cell)
        else {
            return;
        };
        if self.melee.pursuits[index].is_none_or(|pursuit| pursuit.target_id != target_id) {
            self.melee.retry_at[index] = 0;
        }
        if let Some(id) = self.ids[index] {
            let _ = self.world.stop(id);
        }
        self.melee.orders[index] = None;
        self.melee.target_cells[index] = None;
        self.melee.pursuits[index] = Some(DebugPursuit {
            target_id,
            target_cell: (target.column, target.row),
            status: PursuitStatus::Pending,
        });
    }

    fn finish_debug_pursuit(&mut self, index: usize, status: PursuitStatus) {
        if let Some(pursuit) = self.melee.pursuits[index].as_mut() {
            pursuit.status = status;
        }
        if let Some(id) = self.ids[index] {
            let _ = self.world.stop(id);
        }
        self.melee.target_cells[index] = None;
        if matches!(status, PursuitStatus::Refused(PursuitError::NoRoute)) {
            self.melee.retry_at[index] =
                self.tick + u64::from(self.world.rules().repath_cooldown_ticks);
        }
    }

    fn tick_debug_pursuit(&mut self, index: usize) -> bool {
        let pursuit = self.melee.pursuits[index].unwrap();
        let Some(actor) = self
            .debug_actor
            .as_ref()
            .filter(|actor| actor.id == pursuit.target_id)
        else {
            self.finish_debug_pursuit(
                index,
                PursuitStatus::Refused(PursuitError::TargetUnavailable),
            );
            return false;
        };
        let Some(target) = self.world.state(actor.world_id).map(|state| state.cell) else {
            self.finish_debug_pursuit(
                index,
                PursuitStatus::Refused(PursuitError::TargetUnavailable),
            );
            return false;
        };
        if pursuit.status == PursuitStatus::InRange
            && self.melee.target_cells[index] != Some(target)
        {
            // The in-range boundary mirrors state 5 losing its assigned cell (combat.md).
            self.finish_debug_pursuit(index, PursuitStatus::Finished);
            return false;
        }
        let id = self.ids[index].unwrap();
        let state = self.world.state(id).unwrap();
        // Original retention checks the rectangle only after the destination changes.
        if self.melee.target_cells[index].is_some_and(|previous| previous != target)
            && (state.moving || state.goal.is_some())
            && !inside_rectangle(self.melee.leashes[index], target)
        {
            self.finish_debug_pursuit(index, PursuitStatus::Finished);
            return false;
        }
        if state.moving {
            if self.melee.target_cells[index] != Some(target) {
                self.approach_debug_target(index, target);
            }
            return true;
        }
        if let Some(direction) = Direction::between(state.cell, target) {
            let _ = self.world.stop(id);
            let _ = self.world.turn(id, direction);
            self.melee.target_cells[index] = Some(target);
            let pursuit = self.melee.pursuits[index].as_mut().unwrap();
            pursuit.target_cell = (target.column, target.row);
            pursuit.status = PursuitStatus::InRange;
        } else if combat::ambush(self.roster[index].class_id) {
            self.finish_debug_pursuit(index, PursuitStatus::Finished);
            return false;
        } else {
            self.approach_debug_target(index, target);
        }
        true
    }

    fn approach_debug_target(&mut self, index: usize, target: Cell) {
        if self.tick < self.melee.retry_at[index] {
            return;
        }
        match self.request_approach(index, target) {
            Ok(_) => {
                self.melee.target_cells[index] = Some(target);
                let pursuit = self.melee.pursuits[index].as_mut().unwrap();
                pursuit.target_cell = (target.column, target.row);
                pursuit.status = PursuitStatus::Approaching;
            }
            Err(_) => {
                self.finish_debug_pursuit(index, PursuitStatus::Refused(PursuitError::NoRoute));
            }
        }
    }

    fn approach_target(&mut self, index: usize, target: Cell) {
        if self.tick < self.melee.retry_at[index] {
            return;
        }
        match self.request_approach(index, target) {
            Ok(_) => {
                self.melee.target_cells[index] = Some(target);
                self.set_attack_status(index, AttackStatus::Approaching);
            }
            Err(_) => {
                self.finish_attack(index, AttackStatus::Refused(AttackError::NoRoute));
            }
        }
    }

    fn request_approach(&mut self, index: usize, target: Cell) -> Result<usize, WorldError> {
        let id = self.ids[index].unwrap();
        let result = self
            .world
            .move_to_with_policy(id, target, GoalPolicy::Adjacent);
        if let Err(error) = result
            && error != WorldError::NoRoute
        {
            self.order_failures.push((self.roster[index].id, error));
        }
        result
    }

    fn start_retreat(&mut self, index: usize) -> bool {
        let id = self.ids[index].unwrap();
        let cell = self.world.state(id).unwrap().cell;
        let target = k1_game::combat::retreat_target(
            (cell.column as i16, cell.row as i16),
            &self.world,
            &mut self.rng,
        );
        let Some((column, row)) = target else {
            return false;
        };
        if self
            .world
            .move_to_with_policy(id, Cell::new(column as u32, row as u32), GoalPolicy::Exact)
            .is_err()
        {
            return false;
        }
        self.melee.retreating[index] = true;
        self.set_attack_status(index, AttackStatus::Finished);
        true
    }
}

fn inside_rectangle(rectangle: game_state::Rect, cell: Cell) -> bool {
    i32::from(rectangle.left) <= cell.column as i32
        && cell.column as i32 <= i32::from(rectangle.right)
        && i32::from(rectangle.top) <= cell.row as i32
        && cell.row as i32 <= i32::from(rectangle.bottom)
}
