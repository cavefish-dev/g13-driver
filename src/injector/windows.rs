use anyhow::{Context, Result};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
};
use super::{KeyCombo, KeyInjector, Modifier};
use super::key_map::build_key_map;
use std::collections::HashMap;

pub struct WindowsInjector {
    key_map: HashMap<String, u16>,
}

impl WindowsInjector {
    pub fn new() -> Self {
        Self { key_map: build_key_map() }
    }

    fn modifier_vk(m: &Modifier) -> u16 {
        match m {
            Modifier::Ctrl    => 0x11,
            Modifier::Shift   => 0x10,
            Modifier::Alt     => 0xA4,
            Modifier::Windows => 0x5B,
        }
    }

    fn make_input(vk: u16, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }
}

impl KeyInjector for WindowsInjector {
    fn press(&self, combo: &KeyCombo) -> Result<()> {
        let vk = *self.key_map.get(&combo.key)
            .with_context(|| format!("unknown key: {}", combo.key))?;

        let mut inputs: Vec<INPUT> = Vec::new();
        for m in &combo.modifiers {
            inputs.push(Self::make_input(Self::modifier_vk(m), 0));
        }
        inputs.push(Self::make_input(vk, 0));
        inputs.push(Self::make_input(vk, KEYEVENTF_KEYUP));
        for m in combo.modifiers.iter().rev() {
            inputs.push(Self::make_input(Self::modifier_vk(m), KEYEVENTF_KEYUP));
        }

        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        if sent == 0 {
            log::warn!("SendInput returned 0 for combo {:?}", combo);
        }
        Ok(())
    }
}
