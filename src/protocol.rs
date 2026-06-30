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
}

pub struct ReportParser {
    prev_keys: u32,
}

impl ReportParser {
    pub fn new() -> Self {
        Self { prev_keys: 0 }
    }

    pub fn parse(&mut self, report: &[u8; 8]) -> Vec<G13Event> {
        let current = (report[1] as u32)
            | ((report[2] as u32) << 8)
            | ((report[3] as u32) << 16);

        let pressed  = current & !self.prev_keys;
        let released = self.prev_keys & !current;
        self.prev_keys = current;

        let mut events = Vec::new();
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

    fn empty() -> [u8; 8] { [0u8; 8] }

    #[test]
    fn no_keys_no_events() {
        let mut p = ReportParser::new();
        assert!(p.parse(&empty()).is_empty());
    }

    #[test]
    fn g1_press() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[1] = 0b0000_0001;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G1)]);
    }

    #[test]
    fn g1_release() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[1] = 0b0000_0001;
        p.parse(&r);
        assert_eq!(p.parse(&empty()), vec![G13Event::KeyUp(G13Key::G1)]);
    }

    #[test]
    fn g8_press() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[1] = 0b1000_0000;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G8)]);
    }

    #[test]
    fn g9_press() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[2] = 0b0000_0001;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G9)]);
    }

    #[test]
    fn g22_press() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[3] = 0b0010_0000;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G22)]);
    }

    #[test]
    fn two_simultaneous_keys() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[1] = 0b0000_0011;
        let events = p.parse(&r);
        assert_eq!(events.len(), 2);
        assert!(events.contains(&G13Event::KeyDown(G13Key::G1)));
        assert!(events.contains(&G13Event::KeyDown(G13Key::G2)));
    }
}
