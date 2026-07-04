#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum G13Key {
    G1,  G2,  G3,  G4,  G5,  G6,  G7,  G8,
    G9,  G10, G11, G12, G13, G14, G15, G16,
    G17, G18, G19, G20, G21, G22,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum G13Event {
    KeyDown(G13Key),
    KeyUp(G13Key),
    JoystickMove { x: u8, y: u8 },
}

pub struct ReportParser {
    prev_keys: u32,
    prev_x: u8,
    prev_y: u8,
}

impl ReportParser {
    pub fn new() -> Self {
        Self { prev_keys: 0, prev_x: 127, prev_y: 127 }
    }

    pub fn parse(&mut self, report: &[u8; 8]) -> Vec<G13Event> {
        let mut events = Vec::new();

        // Joystick: byte 1 = X, byte 2 = Y (verified on hardware).
        let x = report[1];
        let y = report[2];
        if x != self.prev_x || y != self.prev_y {
            self.prev_x = x;
            self.prev_y = y;
            events.push(G13Event::JoystickMove { x, y });
        }

        // G-key bitmask is bytes 3,4,5 (byte 3 = G1-G8, byte 4 = G9-G16,
        // byte 5 = G17-G22). Bytes 1,2 are the joystick X/Y axes (centered at
        // 0x7F) and byte 5 bit7 is a constant flag — none are keys. Verified
        // against real hardware; see milestones/.../02-hardware-bringup.md.
        let current = (report[3] as u32)
            | ((report[4] as u32) << 8)
            | ((report[5] as u32) << 16);

        let pressed  = current & !self.prev_keys;
        let released = self.prev_keys & !current;
        self.prev_keys = current;

        for bit in 0..22u32 {
            if pressed  & (1 << bit) != 0 { events.push(G13Event::KeyDown(Self::bit_to_key(bit))); }
            if released & (1 << bit) != 0 { events.push(G13Event::KeyUp(Self::bit_to_key(bit))); }
        }
        events
    }

    fn bit_to_key(bit: u32) -> G13Key {
        match bit {
            0  => G13Key::G1,  1  => G13Key::G2,  2  => G13Key::G3,
            3  => G13Key::G4,  4  => G13Key::G5,  5  => G13Key::G6,
            6  => G13Key::G7,  7  => G13Key::G8,  8  => G13Key::G9,
            9  => G13Key::G10, 10 => G13Key::G11, 11 => G13Key::G12,
            12 => G13Key::G13, 13 => G13Key::G14, 14 => G13Key::G15,
            15 => G13Key::G16, 16 => G13Key::G17, 17 => G13Key::G18,
            18 => G13Key::G19, 19 => G13Key::G20, 20 => G13Key::G21,
            21 => G13Key::G22,
            _  => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Idle report captured from real hardware: joystick centered (bytes 1,2 = 0x7F),
    // byte 5 bit7 (0x80) is a constant flag, byte 7 is the joystick button.
    // The key bitmask lives in bytes 3,4,5 — NOT 1,2,3.
    fn idle() -> [u8; 8] { [0x01, 0x7F, 0x7F, 0x00, 0x00, 0x80, 0x00, 0x00] }

    // A centered, no-key-pressed report must emit nothing — neither phantom keys
    // (the bug that misread joystick bytes 1,2 as presses) nor a spurious move.
    #[test]
    fn idle_report_emits_no_events() {
        let mut p = ReportParser::new();
        assert!(p.parse(&idle()).is_empty());
    }

    #[test]
    fn joystick_move_emitted_on_x_change() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[1] = 0x00; // stick full left
        assert_eq!(p.parse(&r), vec![G13Event::JoystickMove { x: 0x00, y: 0x7F }]);
    }

    #[test]
    fn joystick_no_move_when_centered_and_unchanged() {
        let mut p = ReportParser::new();
        p.parse(&idle());                 // first centered report
        assert!(p.parse(&idle()).is_empty()); // unchanged -> no move
    }

    #[test]
    fn key_and_joystick_move_together() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[1] = 0xFF;            // stick full right
        r[3] = 0b0000_0001;     // G1 down
        let events = p.parse(&r);
        assert!(events.contains(&G13Event::JoystickMove { x: 0xFF, y: 0x7F }));
        assert!(events.contains(&G13Event::KeyDown(G13Key::G1)));
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn g1_press() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[3] = 0b0000_0001;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G1)]);
    }

    #[test]
    fn g1_release() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[3] = 0b0000_0001;
        p.parse(&r);
        assert_eq!(p.parse(&idle()), vec![G13Event::KeyUp(G13Key::G1)]);
    }

    #[test]
    fn g8_press() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[3] = 0b1000_0000;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G8)]);
    }

    #[test]
    fn g9_press() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[4] = 0b0000_0001;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G9)]);
    }

    // Real capture for G22: [01,7F,7F,00,00,A0,00,00] — byte5 = 0x80 (flag) | 0x20 (G22).
    #[test]
    fn g22_press() {
        let mut p = ReportParser::new();
        let r = [0x01, 0x7F, 0x7F, 0x00, 0x00, 0xA0, 0x00, 0x00];
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G22)]);
    }

    #[test]
    fn two_simultaneous_keys() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[3] = 0b0000_0011;
        let events = p.parse(&r);
        assert_eq!(events.len(), 2);
        assert!(events.contains(&G13Event::KeyDown(G13Key::G1)));
        assert!(events.contains(&G13Event::KeyDown(G13Key::G2)));
    }

}
