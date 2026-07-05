use std::collections::HashMap;
use std::collections::HashSet;

pub type VKey = u16;

pub fn build_key_map() -> HashMap<String, VKey> {
    let mut m: HashMap<String, VKey> = HashMap::new();

    // a-z → VK_A (0x41) through VK_Z (0x5A)
    for c in b'a'..=b'z' {
        m.insert((c as char).to_string(), (c - b'a' + 0x41) as VKey);
    }
    // 0-9 → VK_0 (0x30) through VK_9 (0x39)
    for c in b'0'..=b'9' {
        m.insert((c as char).to_string(), (c - b'0' + 0x30) as VKey);
    }
    // F1-F24 → 0x70 through 0x87
    for i in 1u16..=24 {
        m.insert(format!("f{i}"), 0x6F + i);
    }

    let specials: &[(&str, VKey)] = &[
        ("enter",       0x0D), ("return",       0x0D),
        ("esc",         0x1B), ("escape",        0x1B),
        ("space",       0x20), ("tab",           0x09),
        ("backspace",   0x08), ("delete",        0x2E),
        ("insert",      0x2D), ("home",          0x24),
        ("end",         0x23), ("pageup",        0x21),
        ("pagedown",    0x22), ("up",            0x26),
        ("down",        0x28), ("left",          0x25),
        ("right",       0x27), ("capslock",      0x14),
        ("printscreen", 0x2C), ("pause",         0x13),
        ("numlock",     0x90), ("scrolllock",    0x91),
        ("ctrl",        0x11), ("control",       0x11),
        ("shift",       0x10), ("alt",           0xA4),
        ("windows",     0x5B), ("win",           0x5B),
        // Multimedia keys (tap-only).
        ("playpause",   0xB3),
        ("nexttrack",   0xB0), ("next",         0xB0),
        ("prevtrack",   0xB1), ("prev",         0xB1),
        ("mediastop",   0xB2),
        ("volup",       0xAF), ("volumeup",     0xAF),
        ("voldown",     0xAE), ("volumedown",   0xAE),
        ("mute",        0xAD),
    ];
    for (name, vk) in specials {
        m.insert(name.to_string(), *vk);
    }
    m
}

/// Names of keys that should tap (down+up on press) rather than hold — the
/// multimedia keys, where holding is meaningless. Everything else holds.
pub fn tap_only_keys() -> HashSet<String> {
    ["playpause", "nexttrack", "next", "prevtrack", "prev", "mediastop",
     "volup", "volumeup", "voldown", "volumedown", "mute"]
        .iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_a_to_z() {
        let m = build_key_map();
        assert_eq!(m["a"], 0x41);
        assert_eq!(m["z"], 0x5A);
        assert_eq!(m["m"], 0x4D);
    }

    #[test]
    fn digits_0_to_9() {
        let m = build_key_map();
        assert_eq!(m["0"], 0x30);
        assert_eq!(m["9"], 0x39);
    }

    #[test]
    fn function_keys() {
        let m = build_key_map();
        assert_eq!(m["f1"],  0x70);
        assert_eq!(m["f12"], 0x7B);
        assert_eq!(m["f24"], 0x87);
    }

    #[test]
    fn special_keys() {
        let m = build_key_map();
        assert_eq!(m["enter"],     0x0D);
        assert_eq!(m["esc"],       0x1B);
        assert_eq!(m["backspace"], 0x08);
        assert_eq!(m["delete"],    0x2E);
        assert_eq!(m["pageup"],    0x21);
        assert_eq!(m["up"],        0x26);
    }

    #[test]
    fn modifier_keys() {
        let m = build_key_map();
        assert_eq!(m["ctrl"],    0x11);
        assert_eq!(m["shift"],   0x10);
        assert_eq!(m["alt"],     0xA4);
        assert_eq!(m["windows"], 0x5B);
    }

    #[test]
    fn unknown_key_absent() {
        let m = build_key_map();
        assert!(!m.contains_key("xyzzy"));
    }

    #[test]
    fn media_keys_present() {
        let m = build_key_map();
        assert_eq!(m["playpause"], 0xB3);
        assert_eq!(m["nexttrack"], 0xB0);
        assert_eq!(m["next"], 0xB0);
        assert_eq!(m["prevtrack"], 0xB1);
        assert_eq!(m["volup"], 0xAF);
        assert_eq!(m["voldown"], 0xAE);
        assert_eq!(m["mute"], 0xAD);
        assert_eq!(m["mediastop"], 0xB2);
    }

    #[test]
    fn tap_only_is_media_only() {
        let t = tap_only_keys();
        assert!(t.contains("playpause"));
        assert!(t.contains("volup"));
        assert!(t.contains("mute"));
        assert!(!t.contains("a"));
        assert!(!t.contains("shift"));
        assert!(!t.contains("f5"));
    }
}
