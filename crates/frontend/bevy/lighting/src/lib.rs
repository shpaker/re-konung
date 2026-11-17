//! Surface contract and independent Bevy lighting implementations.

use bevy::prelude::*;

pub mod classic;

#[derive(Component, Debug, Clone, Copy)]
pub struct LightingSurface;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LightingSet {
    Apply,
}
