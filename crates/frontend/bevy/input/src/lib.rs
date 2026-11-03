//! Input mechanisms for the «Князь» frontends: keyboard direction, mouse drag,
//! wheel accumulation.
//!
//! No camera types, no UI, no game rules — the mechanisms are plain functions
//! and one accumulator, and each app binds them to its own camera.

use bevy::{
    input::mouse::{AccumulatedMouseMotion, MouseScrollUnit, MouseWheel},
    prelude::*,
};

/// How many scroll "lines" give one zoom step: one wheel notch.
pub const LINES_PER_ZOOM_STEP: f32 = 1.0;
/// How many scrolled pixels give one zoom step: a short trackpad gesture.
///
/// At 100 pixels a trackpad gesture did not respond at once: the first events
/// did not reach even one step, and the zoom lagged behind the scroll.
pub const PIXELS_PER_ZOOM_STEP: f32 = 60.0;
/// How many zoom steps one frame gives out.
///
/// The cap is needed, otherwise gesture inertia scrolls through the whole
/// range; but it must be counted per frame, not per event: the wheel sends
/// several notches in one event, and the former per-event limit collapsed them
/// into a single step.
pub const MAX_ZOOM_STEPS_PER_FRAME: i32 = 3;
/// How many steps survive waiting for their turn in the queue.
///
/// A long gesture spreads over frames but does not accumulate forever: beyond
/// this limit the excess is discarded anyway — as before, only much later.
const ZOOM_ACCUM_LIMIT: f32 = 6.0;

/// Scroll accumulator for zoom.
///
/// One step requires a threshold, and an unfinished fraction is not reset:
/// [ZoomScroll::push] accumulates scroll, [ZoomScroll::take] gives out no more
/// than [`MAX_ZOOM_STEPS_PER_FRAME`] per frame, and the remainder carries into
/// the next frame.
#[derive(Debug, Default)]
pub struct ZoomScroll {
    accum: f32,
}

impl ZoomScroll {
    /// Accounts for the next scroll event.
    pub fn push(&mut self, unit: MouseScrollUnit, y: f32) {
        let normalized = match unit {
            MouseScrollUnit::Line => y / LINES_PER_ZOOM_STEP,
            MouseScrollUnit::Pixel => y / PIXELS_PER_ZOOM_STEP,
        };
        self.accum = (self.accum + normalized).clamp(-ZOOM_ACCUM_LIMIT, ZOOM_ACCUM_LIMIT);
    }

    /// Takes this frame's steps: no more than the cap, the remainder stays.
    pub fn take(&mut self) -> i32 {
        let whole = self.accum.trunc();
        let steps = whole.clamp(
            -(MAX_ZOOM_STEPS_PER_FRAME as f32),
            MAX_ZOOM_STEPS_PER_FRAME as f32,
        );
        self.accum -= steps;
        steps as i32
    }

    /// Forgets the whole accumulation.
    ///
    /// Needed when the cursor is over the interface: a scroll that happened
    /// over a panel must not reach the map later, and `take` would leave it
    /// waiting for the next frame.
    pub fn clear(&mut self) {
        self.accum = 0.0;
    }
}

/// WASD and arrow keys as a world-space direction (y up); zero when nothing is
/// pressed.
pub fn pan_direction(keys: &ButtonInput<KeyCode>) -> Vec2 {
    let mut direction = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    direction
}

/// Left-button drag in screen pixels (x right, y DOWN); None when not dragging.
pub fn drag_delta(
    buttons: &ButtonInput<MouseButton>,
    motion: &AccumulatedMouseMotion,
) -> Option<Vec2> {
    buttons.pressed(MouseButton::Left).then_some(motion.delta)
}

/// Wheel steps for this frame: events are always consumed (so they never pile
/// up), the queue burns down entirely over the UI, and at most
/// [`MAX_ZOOM_STEPS_PER_FRAME`] steps leave per frame.
pub fn wheel_steps(
    scroll: &mut ZoomScroll,
    wheel: &mut MessageReader<MouseWheel>,
    over_ui: bool,
) -> i32 {
    for event in wheel.read() {
        scroll.push(event.unit, event.y);
    }
    if over_ui {
        scroll.clear();
        return 0;
    }
    scroll.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scroll events — one per frame — return the steps the frames really get:
    /// each next `take` picks up what is left in the queue after the last
    /// event, with no new events.
    fn scroll_events(events: &[(MouseScrollUnit, f32)]) -> Vec<i32> {
        let mut scroll = ZoomScroll::default();
        let mut frames = Vec::new();
        for &(unit, y) in events {
            scroll.push(unit, y);
            frames.push(scroll.take());
        }
        loop {
            let taken = scroll.take();
            if taken == 0 {
                break;
            }
            frames.push(taken);
        }
        frames.retain(|steps| *steps != 0);
        frames
    }

    #[test]
    fn one_wheel_notch_is_one_level() {
        assert_eq!(scroll_events(&[(MouseScrollUnit::Line, 1.0)]), [1]);
        assert_eq!(scroll_events(&[(MouseScrollUnit::Line, -1.0)]), [-1]);
    }

    #[test]
    fn fractional_scrolls_accumulate_to_a_level() {
        // Halves do not give a step until they add up to a whole one
        assert_eq!(
            scroll_events(&[(MouseScrollUnit::Line, 0.5), (MouseScrollUnit::Line, 0.5)]),
            [1]
        );
    }

    #[test]
    fn trackpad_swipe_is_bounded() {
        // A swipe: 30 events of 10 pixels = 300 pixels = five steps, not
        // thirty; no more than the cap per frame, the remainder arrives later
        let events = vec![(MouseScrollUnit::Pixel, 10.0); 30];
        let per_frame = scroll_events(&events);
        assert_eq!(per_frame.iter().sum::<i32>(), 5, "a 300-pixel swipe");
        assert!(
            per_frame
                .iter()
                .all(|steps| steps.abs() <= MAX_ZOOM_STEPS_PER_FRAME),
            "{per_frame:?} — more than the cap in one frame"
        );
    }

    #[test]
    fn a_fast_scroll_spreads_over_frames_instead_of_vanishing() {
        // The wheel on macOS sends several notches in one event. The former
        // per-event cap collapsed them into a single step; now the cap is per
        // frame, and the next event is not discarded but waits for its frame
        assert_eq!(scroll_events(&[(MouseScrollUnit::Line, 2.0)]), [2]);
        assert_eq!(
            scroll_events(&[(MouseScrollUnit::Line, 3.0), (MouseScrollUnit::Line, 3.0)]),
            [3, 3]
        );
    }

    #[test]
    fn unfinished_pixels_carry_to_the_next_event() {
        // 60 pixels per step: two unfinished gestures add up to a step
        let mut scroll = ZoomScroll::default();
        scroll.push(MouseScrollUnit::Pixel, 40.0);
        assert_eq!(scroll.take(), 0);
        scroll.push(MouseScrollUnit::Pixel, 40.0);
        assert_eq!(scroll.take(), 1);
        scroll.push(MouseScrollUnit::Pixel, 40.0);
        assert_eq!(scroll.take(), 1);
    }

    #[test]
    fn scroll_direction_is_kept() {
        assert_eq!(
            scroll_events(&[
                (MouseScrollUnit::Line, -1.0),
                (MouseScrollUnit::Pixel, -60.0)
            ]),
            [-1, -1]
        );
    }

    #[test]
    fn opposite_scrolls_cancel_out() {
        let mut scroll = ZoomScroll::default();
        // An unfinished threshold in one direction is cancelled by a scroll in
        // the other
        scroll.push(MouseScrollUnit::Line, 0.5);
        scroll.push(MouseScrollUnit::Line, -0.5);
        assert_eq!(scroll.take(), 0);
    }

    #[test]
    fn scrolling_over_the_interface_does_not_reach_the_map_later() {
        // Scenario from a review: six steps over a panel, then the cursor moves
        // to the map. take() would return three of them — so over the interface
        // the queue burns down entirely
        let mut scroll = ZoomScroll::default();
        for _ in 0..6 {
            scroll.push(MouseScrollUnit::Line, 1.0);
        }
        scroll.clear();
        assert_eq!(scroll.take(), 0);

        // The fractional remainder burns the same way: half a step over a panel
        // must not add up with the next gesture, already over the map
        scroll.push(MouseScrollUnit::Line, 0.5);
        scroll.clear();
        scroll.push(MouseScrollUnit::Line, 0.5);
        assert_eq!(scroll.take(), 0);
    }

    #[test]
    fn pan_direction_reads_wasd_and_arrows() {
        let mut keys = ButtonInput::default();
        assert_eq!(pan_direction(&keys), Vec2::ZERO);
        keys.press(KeyCode::KeyW);
        assert_eq!(pan_direction(&keys), Vec2::new(0.0, 1.0));
        keys.reset_all();
        keys.press(KeyCode::KeyD);
        assert_eq!(pan_direction(&keys), Vec2::new(1.0, 0.0));
        keys.reset_all();
        keys.press(KeyCode::ArrowLeft);
        assert_eq!(pan_direction(&keys), Vec2::new(-1.0, 0.0));
        keys.reset_all();
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::KeyD);
        assert_eq!(pan_direction(&keys), Vec2::new(1.0, 1.0));
    }

    #[test]
    fn drag_delta_exists_only_while_the_left_button_is_down() {
        let motion = AccumulatedMouseMotion {
            delta: Vec2::new(3.0, -2.0),
        };
        let mut buttons = ButtonInput::default();
        assert_eq!(drag_delta(&buttons, &motion), None);
        buttons.press(MouseButton::Left);
        assert_eq!(drag_delta(&buttons, &motion), Some(Vec2::new(3.0, -2.0)));
    }
}
