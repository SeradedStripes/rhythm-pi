use std::collections::HashMap;

/// Defines which keys control which lanes
#[derive(Debug, Clone)]
pub struct KeyBindings {
    pub lane_1: char, // Default: 'D'
    pub lane_2: char, // Default: 'F'
    pub lane_3: char, // Default: 'J'
    pub lane_4: char, // Default: 'K'
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            lane_1: 'A',
            lane_2: 'S',
            lane_3: 'J',
            lane_4: 'K',
        }
    }
}

impl KeyBindings {
    /// Map a key to its corresponding lane (0-3)
    /// Returns None if the key is not bound
    pub fn key_to_lane(&self, key: char) -> Option<u32> {
        match key.to_ascii_uppercase() {
            c if c == self.lane_1.to_ascii_uppercase() => Some(0),
            c if c == self.lane_2.to_ascii_uppercase() => Some(1),
            c if c == self.lane_3.to_ascii_uppercase() => Some(2),
            c if c == self.lane_4.to_ascii_uppercase() => Some(3),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InputEvent {
    pub lane: u32,
    pub timestamp: f32,
    pub key: char,
}

pub struct InputHandler {
    bindings: KeyBindings,
    active_keys: HashMap<char, bool>,
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

    pub fn handle_key_press(&mut self, key: char, current_time: f32) -> Option<InputEvent> {
        if let Some(lane) = self.bindings.key_to_lane(key) {
            self.active_keys.insert(key, true);
            Some(InputEvent {
                lane,
                timestamp: current_time,
                key,
            })
        } else {
            None
        }
    }

    pub fn handle_key_release(&mut self, key: char) {
        self.active_keys.remove(&key);
    }

    pub fn is_key_pressed(&self, key: char) -> bool {
        self.active_keys.get(&key).copied().unwrap_or(false)
    }

    pub fn is_lane_pressed(&self, lane: u32) -> bool {
        match lane {
            0 => self.is_key_pressed(self.bindings.lane_1),
            1 => self.is_key_pressed(self.bindings.lane_2),
            2 => self.is_key_pressed(self.bindings.lane_3),
            3 => self.is_key_pressed(self.bindings.lane_4),
            _ => false,
        }
    }

    pub fn set_bindings(&mut self, bindings: KeyBindings) {
        self.bindings = bindings;
        self.active_keys.clear();
    }

    pub fn get_bindings(&self) -> &KeyBindings {
        &self.bindings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_to_lane() {
        let bindings = KeyBindings::default();
        assert_eq!(bindings.key_to_lane('a'), Some(0));
        assert_eq!(bindings.key_to_lane('s'), Some(1));
        assert_eq!(bindings.key_to_lane('j'), Some(2));
        assert_eq!(bindings.key_to_lane('k'), Some(3));
        assert_eq!(bindings.key_to_lane('x'), None);
    }

    #[test]
    fn test_input_handler() {
        let mut handler = InputHandler::with_default_bindings();
        
        let event = handler.handle_key_press('a', 1.5);
        assert!(event.is_some());
        
        let event = event.unwrap();
        assert_eq!(event.lane, 0);
        assert_eq!(event.timestamp, 1.5);
        
        assert!(handler.is_key_pressed('a'));
        assert!(handler.is_lane_pressed(0));
        
        handler.handle_key_release('a');
        assert!(!handler.is_key_pressed('a'));
    }
}
