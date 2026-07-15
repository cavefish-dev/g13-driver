use crate::config::JoystickConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldAction {
    KeyDown(String),
    KeyUp(String),
}

/// Converts analog joystick X/Y into key-hold transitions using independent
/// per-axis thresholding (8-way: a diagonal holds two keys). Holds only the
/// current held-key state; deadzone and key bindings are read from the config
/// passed to `update`, so config hot-reload takes effect live.
pub struct JoystickMapper {
    x_held: Option<String>,
    y_held: Option<String>,
}

const CENTER: i32 = 127;

impl JoystickMapper {
    pub fn new() -> Self {
        Self { x_held: None, y_held: None }
    }

    pub fn update(&mut self, x: u8, y: u8, cfg: &JoystickConfig, deadzone: u8) -> Vec<HoldAction> {
        let mut actions = Vec::new();
        let want_x = Self::target(x, deadzone, &cfg.left, &cfg.right);
        Self::diff(&mut actions, &mut self.x_held, want_x);
        let want_y = Self::target(y, deadzone, &cfg.up, &cfg.down);
        Self::diff(&mut actions, &mut self.y_held, want_y);
        actions
    }

    pub fn release_all(&mut self) -> Vec<HoldAction> {
        let mut actions = Vec::new();
        if let Some(k) = self.x_held.take() { actions.push(HoldAction::KeyUp(k)); }
        if let Some(k) = self.y_held.take() { actions.push(HoldAction::KeyUp(k)); }
        actions
    }

    /// Which key (if any) a single axis wants held, given its low/high targets.
    fn target(value: u8, deadzone: u8, low: &Option<String>, high: &Option<String>) -> Option<String> {
        let v = value as i32;
        let dz = deadzone as i32;
        if v < CENTER - dz {
            low.clone()
        } else if v > CENTER + dz {
            high.clone()
        } else {
            None
        }
    }

    /// Emit transitions to move one axis from its current held key to `want`.
    fn diff(actions: &mut Vec<HoldAction>, held: &mut Option<String>, want: Option<String>) {
        if *held == want {
            return;
        }
        if let Some(k) = held.take() {
            actions.push(HoldAction::KeyUp(k));
        }
        if let Some(k) = &want {
            actions.push(HoldAction::KeyDown(k.clone()));
        }
        *held = want;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::JoystickConfig;

    fn wasd() -> JoystickConfig {
        JoystickConfig {
            up: Some("w".into()),
            down: Some("s".into()),
            left: Some("a".into()),
            right: Some("d".into()),
        }
    }

    #[test]
    fn centered_emits_nothing() {
        let mut m = JoystickMapper::new();
        assert!(m.update(127, 127, &wasd(), 30).is_empty());
    }

    #[test]
    fn inside_deadzone_emits_nothing() {
        let mut m = JoystickMapper::new();
        // deadzone 30 -> fires only below 97 or above 157
        assert!(m.update(100, 150, &wasd(), 30).is_empty());
    }

    #[test]
    fn full_left_presses_a() {
        let mut m = JoystickMapper::new();
        assert_eq!(m.update(0, 127, &wasd(), 30), vec![HoldAction::KeyDown("a".into())]);
    }

    #[test]
    fn full_right_presses_d() {
        let mut m = JoystickMapper::new();
        assert_eq!(m.update(255, 127, &wasd(), 30), vec![HoldAction::KeyDown("d".into())]);
    }

    #[test]
    fn full_up_presses_w() {
        let mut m = JoystickMapper::new();
        assert_eq!(m.update(127, 0, &wasd(), 30), vec![HoldAction::KeyDown("w".into())]);
    }

    #[test]
    fn full_down_presses_s() {
        let mut m = JoystickMapper::new();
        assert_eq!(m.update(127, 255, &wasd(), 30), vec![HoldAction::KeyDown("s".into())]);
    }

    #[test]
    fn return_to_center_releases() {
        let mut m = JoystickMapper::new();
        m.update(0, 127, &wasd(), 30);                    // hold a
        assert_eq!(m.update(127, 127, &wasd(), 30), vec![HoldAction::KeyUp("a".into())]);
    }

    #[test]
    fn diagonal_holds_two_keys() {
        let mut m = JoystickMapper::new();
        let actions = m.update(0, 0, &wasd(), 30);        // up-left
        assert!(actions.contains(&HoldAction::KeyDown("a".into())));
        assert!(actions.contains(&HoldAction::KeyDown("w".into())));
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn cross_center_left_to_right_swaps_without_stuck_key() {
        let mut m = JoystickMapper::new();
        m.update(0, 127, &wasd(), 30);                    // hold a
        let actions = m.update(255, 127, &wasd(), 30);    // jump full right
        assert_eq!(actions, vec![
            HoldAction::KeyUp("a".into()),
            HoldAction::KeyDown("d".into()),
        ]);
    }

    #[test]
    fn holding_in_zone_is_idempotent() {
        let mut m = JoystickMapper::new();
        m.update(0, 127, &wasd(), 30);                    // hold a
        assert!(m.update(10, 127, &wasd(), 30).is_empty()); // still left, no new event
    }

    #[test]
    fn release_all_lifts_held_keys() {
        let mut m = JoystickMapper::new();
        m.update(0, 0, &wasd(), 30);                      // hold a + w
        let mut released = m.release_all();
        released.sort_by(|x, y| format!("{:?}", x).cmp(&format!("{:?}", y)));
        assert_eq!(released, vec![
            HoldAction::KeyUp("a".into()),
            HoldAction::KeyUp("w".into()),
        ]);
        assert!(m.release_all().is_empty());            // second call: nothing
    }

    #[test]
    fn unmapped_direction_emits_nothing() {
        let mut cfg = wasd();
        cfg.up = None;
        let mut m = JoystickMapper::new();
        assert!(m.update(127, 0, &cfg, 30).is_empty());     // up is unmapped
    }

    #[test]
    fn deadzone_param_gates_direction() {
        let mut j = JoystickMapper::new();
        let cfg = JoystickConfig { up: Some("w".into()), down: Some("s".into()),
                                   left: Some("a".into()), right: Some("d".into()) };
        // within deadzone (center ± 40 with dz=50) → no action
        let actions = j.update(127 - 40, 127, &cfg, 50);
        assert!(actions.is_empty());
    }
}
