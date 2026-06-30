// stub — implemented in Task 7
use anyhow::Result;
use super::{KeyCombo, KeyInjector};
pub struct WindowsInjector;
impl WindowsInjector { pub fn new() -> Self { Self } }
impl KeyInjector for WindowsInjector {
    fn press(&self, _combo: &KeyCombo) -> Result<()> { Ok(()) }
}
