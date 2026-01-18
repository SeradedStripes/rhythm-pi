use std::collections::HashMap;
use log::info;

/// Defines which keys control which lanes (case-insensitive)
/// Default layout: D F J K
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBindings {
    pub lane_1: char,
    pub lane_2: char,
    pub lane_3: char,
    pub lane_4: char,
}

impl KeyBindings {
    /// Create a new `KeyBindings` from 4 lane keys. This enforces case-insensitive
    /// uniqueness of keys (e.g., 'd' and 'D' are the same key).
    pub fn new(l1: char, l2: char, l3: char, l4: char) -> Self {
        Self {
            lane_1: l1,
            lane_2: l2,
            lane_3: l3,
            lane_4: l4,
        }
    }

    /// Map a key to its corresponding lane (0-3). Returns None if not bound.
    pub fn key_to_lane(&self, key: char) -> Option<u32> {
        let k = key.to_ascii_uppercase();
        if k == self.lane_1.to_ascii_uppercase() {
            Some(0)
        } else if k == self.lane_2.to_ascii_uppercase() {
            Some(1)
        } else if k == self.lane_3.to_ascii_uppercase() {
            Some(2)
        } else if k == self.lane_4.to_ascii_uppercase() {
            Some(3)
        } else {
            None
        }
    }

    /// Return the key for a lane, or None if lane is out of range
    pub fn lane_to_key(&self, lane: u32) -> Option<char> {
        match lane {
            0 => Some(self.lane_1),
            1 => Some(self.lane_2),
            2 => Some(self.lane_3),
            3 => Some(self.lane_4),
            _ => None,
        }
    }

    /// Attempt to set a binding for a lane. Returns Err if the key is already bound
    /// to another lane (case-insensitive).
    pub fn set_binding(&mut self, lane: u32, key: char) -> Result<(), String> {
        let k = key.to_ascii_uppercase();
        // Check uniqueness
        for (idx, existing) in [self.lane_1, self.lane_2, self.lane_3, self.lane_4].iter().enumerate() {
            if (*existing).to_ascii_uppercase() == k && (idx as u32) != lane {
                return Err(format!("Key '{}' is already bound to lane {}", k, idx));
            }
        }

        match lane {
            0 => self.lane_1 = key,
            1 => self.lane_2 = key,
            2 => self.lane_3 = key,
            3 => self.lane_4 = key,
            _ => return Err(format!("Invalid lane {}", lane)),
        }

        Ok(())
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::new('D', 'F', 'J', 'K')
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputEvent {
    pub lane: u32,
    pub timestamp: f32,
    pub key: char,
}

/// Handles input press/release state and timestamps
pub struct InputHandler {
    bindings: KeyBindings,
    active_keys: HashMap<char, f32>, // uppercase char -> press time
}

impl InputHandler {
    pub fn new(bindings: KeyBindings) -> Self {
        Self {
            bindings,
            active_keys: HashMap::new(),
        }
    }

    pub fn with_default_bindings() -> Self {
        Self::new(KeyBindings::default())
    }

    /// Handle a key press. Returns Some(InputEvent) only when the key transitions
    /// from not-pressed -> pressed (repeat presses while held are ignored).
    pub fn handle_key_press(&mut self, key: char, current_time: f32) -> Option<InputEvent> {
        if let Some(lane) = self.bindings.key_to_lane(key) {
            let k = key.to_ascii_uppercase();
            if self.active_keys.contains_key(&k) {
                // already pressed; ignore repeated press
                info!("handle_key_press: key='{}' lane={} already pressed", k, lane);
                return None;
            }
            self.active_keys.insert(k, current_time);
            info!("handle_key_press: key='{}' lane={} time={:.3}", k, lane, current_time);
            Some(InputEvent { lane, timestamp: current_time, key: k })
        } else {
            None
        }
    }

    /// Handle key release. If release_time is Some(t), returns an InputEvent at time t.
    /// Removes the key from pressed state regardless of whether a timestamp is provided.
    pub fn handle_key_release(&mut self, key: char, release_time: Option<f32>) -> Option<InputEvent> {
        let k = key.to_ascii_uppercase();
        match self.active_keys.remove(&k) {
            Some(press_time) => {
                info!("handle_key_release: key='{}' pressed_at={:.3}", k, press_time);
                if let Some(t) = release_time {
                    if let Some(lane) = self.bindings.key_to_lane(k) {
                        return Some(InputEvent { lane, timestamp: t, key: k });
                    }
                }
                None
            }
            None => {
                info!("handle_key_release: key='{}' not pressed", k);
                None
            }
        }
    }

    pub fn is_key_pressed(&self, key: char) -> bool {
        self.active_keys.contains_key(&key.to_ascii_uppercase())
    }

    pub fn is_lane_pressed(&self, lane: u32) -> bool {
        match self.bindings.lane_to_key(lane) {
            Some(k) => self.is_key_pressed(k),
            None => false,
        }
    }

    /// Replace all bindings (clears pressed state)
    pub fn set_bindings(&mut self, bindings: KeyBindings) {
        self.bindings = bindings;
        self.active_keys.clear();
    }

    pub fn get_bindings(&self) -> &KeyBindings {
        &self.bindings
    }

    /// Return currently pressed lanes
    pub fn pressed_lanes(&self) -> Vec<u32> {
        let mut lanes = Vec::new();
        for (k, _) in self.active_keys.iter() {
            if let Some(l) = self.bindings.key_to_lane(*k) {
                lanes.push(l);
            }
        }
        lanes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_bindings() {
        let bindings = KeyBindings::default();
        assert_eq!(bindings.key_to_lane('d'), Some(0));
        assert_eq!(bindings.key_to_lane('f'), Some(1));
        assert_eq!(bindings.key_to_lane('j'), Some(2));
        assert_eq!(bindings.key_to_lane('k'), Some(3));
    }

    #[test]
    fn test_case_insensitive_mapping() {
        let bindings = KeyBindings::default();
        assert_eq!(bindings.key_to_lane('D'), Some(0));
        assert_eq!(bindings.key_to_lane('d'), Some(0));
    }

    #[test]
    fn test_input_handler_press_release() {
        let mut handler = InputHandler::with_default_bindings();
        // Press once
        let event = handler.handle_key_press('d', 1.5);
        assert!(event.is_some());
        let e = event.unwrap();
        assert_eq!(e.lane, 0);
        assert_eq!(e.timestamp, 1.5);
        assert!(handler.is_key_pressed('d'));
        assert!(handler.is_lane_pressed(0));

        // Repeated press while held should be ignored (no new event)
        assert!(handler.handle_key_press('d', 1.6).is_none());

        // Release without timestamp clears pressed state
        handler.handle_key_release('d', None);
        assert!(!handler.is_key_pressed('d'));
    }

    #[test]
    fn test_release_generates_event_when_time_provided() {
        let mut handler = InputHandler::with_default_bindings();
        let _ = handler.handle_key_press('d', 1.0);
        let maybe_event = handler.handle_key_release('d', Some(1.12));
        assert!(maybe_event.is_some());
        let ev = maybe_event.unwrap();
        assert_eq!(ev.lane, 0);
        assert_eq!(ev.timestamp, 1.12);
    }

    #[test]
    fn test_set_binding_uniqueness() {
        let mut b = KeyBindings::default();
        // Try binding lane 1 to 'D' which is already lane 0
        assert!(b.set_binding(1, 'd').is_err());
        // Valid change
        assert!(b.set_binding(1, 's').is_ok());
        assert_eq!(b.key_to_lane('s'), Some(1));
    }
}
