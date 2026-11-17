//! Serializable pursuit state and facade inspection snapshots.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DebugPursuit {
    pub target_id: u64,
    /// Last assigned target cell, retained after completion for inspection.
    pub target_cell: (u32, u32),
    pub status: PursuitStatus,
}

impl From<k1_game::simulation::DebugPursuit> for DebugPursuit {
    fn from(value: k1_game::simulation::DebugPursuit) -> Self {
        use k1_game::simulation::{PursuitError as Error, PursuitStatus as Status};
        Self {
            target_id: value.target_id,
            target_cell: value.target_cell,
            status: match value.status {
                Status::Pending => PursuitStatus::Pending,
                Status::Approaching => PursuitStatus::Approaching,
                Status::InRange => PursuitStatus::InRange,
                Status::Finished => PursuitStatus::Finished,
                Status::Refused(error) => PursuitStatus::Refused(match error {
                    Error::TargetUnavailable => PursuitError::TargetUnavailable,
                    Error::NoRoute => PursuitError::NoRoute,
                }),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreatureInspection {
    pub id: usize,
    pub label: String,
    pub recorded_state: u8,
    pub animation_state: Option<u8>,
    pub animation_stage: Option<k1_game::creature_anim::Stage>,
    pub walk_state: Option<k1_game::simulation::WalkState>,
    pub group: usize,
    pub group_state: k1_game::rules::GroupState,
    pub group_state_tick: Option<u64>,
    pub attack: Option<k1_game::simulation::AttackOrder>,
    pub pursuit: Option<DebugPursuit>,
}

impl From<k1_game::simulation::CreatureInspection> for CreatureInspection {
    fn from(value: k1_game::simulation::CreatureInspection) -> Self {
        Self {
            id: value.id,
            label: value.label,
            recorded_state: value.recorded_state,
            animation_state: value.animation_state,
            animation_stage: value.animation_stage,
            walk_state: value.walk_state,
            group: value.group,
            group_state: value.group_state,
            group_state_tick: value.group_state_tick,
            attack: value.attack,
            pursuit: value.pursuit.map(Into::into),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k1_game::simulation::{
        DebugPursuit as RuntimePursuit, PursuitError as RuntimeError,
        PursuitStatus as RuntimeStatus,
    };

    #[test]
    fn native_pursuit_keeps_the_replay_snapshot_json_contract() {
        let cases = [
            (RuntimeStatus::Pending, serde_json::json!("pending")),
            (RuntimeStatus::Approaching, serde_json::json!("approaching")),
            (RuntimeStatus::InRange, serde_json::json!("in_range")),
            (RuntimeStatus::Finished, serde_json::json!("finished")),
            (
                RuntimeStatus::Refused(RuntimeError::NoRoute),
                serde_json::json!({"refused": "no_route"}),
            ),
            (
                RuntimeStatus::Refused(RuntimeError::TargetUnavailable),
                serde_json::json!({"refused": "target_unavailable"}),
            ),
        ];
        for (status, expected) in cases {
            let pursuit = DebugPursuit::from(RuntimePursuit {
                target_id: 17,
                target_cell: (2, 3),
                status,
            });
            let value = serde_json::to_value(pursuit).unwrap();
            assert_eq!(
                value,
                serde_json::json!({"target_id":17, "target_cell":[2,3], "status": expected})
            );
            assert_eq!(
                serde_json::from_value::<DebugPursuit>(value).unwrap(),
                pursuit
            );
        }
    }
}
