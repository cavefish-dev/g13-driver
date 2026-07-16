use anyhow::{Context, Result};
use rusb::{DeviceHandle, GlobalContext};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use crate::led::{LedState, color_packet, mkey_packet};
use crate::protocol::{G13Event, ReportParser};

const G13_VID: u16 = 0x046D;
const G13_PID: u16 = 0xC21C;
const ENDPOINT_IN: u8 = 0x81;
const READ_TIMEOUT: Duration = Duration::from_millis(100);
// SET_REPORT: host->device, class, interface recipient.
const LED_REQUEST_TYPE: u8 = 0x21;
const LED_REQUEST: u8 = 0x09;
const LED_COLOR_VALUE: u16 = 0x0307;
const LED_MKEY_VALUE: u16 = 0x0305;
const LED_TIMEOUT: Duration = Duration::from_millis(100);

pub struct UsbReader {
    handle: DeviceHandle<GlobalContext>,
}

impl UsbReader {
    pub fn open() -> Result<Self> {
        let handle = rusb::open_device_with_vid_pid(G13_VID, G13_PID)
            .context("G13 not found — plug it in and run Zadig to install WinUSB (see docs/zadig-setup.md)")?;
        handle.claim_interface(0)
            .context("failed to claim USB interface 0 — is another driver already attached?")?;
        Ok(Self { handle })
    }

    pub fn run(mut self, tx: Sender<G13Event>, desired: Arc<Mutex<LedState>>) -> Result<()> {
        let mut parser = ReportParser::new();
        let mut buf = [0u8; 8];
        // None so the first tick always applies the current desired state (also
        // re-applies after a reconnect, since `run` is called fresh each time).
        let mut last_applied: Option<LedState> = None;
        loop {
            self.apply_leds(&desired, &mut last_applied);
            match self.handle.read_interrupt(ENDPOINT_IN, &mut buf, READ_TIMEOUT) {
                Ok(8) => {
                    log::trace!("raw report: {buf:02X?}");
                    for event in parser.parse(&buf) {
                        if tx.send(event).is_err() {
                            return Ok(());
                        }
                    }
                }
                Ok(n) => log::warn!("unexpected report size: {n} bytes"),
                Err(rusb::Error::Timeout) => continue,
                Err(e) => {
                    return Err(anyhow::Error::from(e).context("USB read error"));
                }
            }
        }
    }

    /// Apply the desired LED state to the device when it changed. Warn-and-continue
    /// on transfer errors (a missed LED update is recoverable).
    fn apply_leds(&self, desired: &Arc<Mutex<LedState>>, last: &mut Option<LedState>) {
        let want = *desired.lock().unwrap();
        if *last == Some(want) {
            return;
        }
        let color = color_packet(want.rgb);
        if let Err(e) = self.handle.write_control(
            LED_REQUEST_TYPE, LED_REQUEST, LED_COLOR_VALUE, 0, &color, LED_TIMEOUT) {
            log::warn!("backlight color write failed: {e}");
            return; // leave `last` unchanged so we retry next tick
        }
        let mkeys = mkey_packet(want.mkeys);
        if let Err(e) = self.handle.write_control(
            LED_REQUEST_TYPE, LED_REQUEST, LED_MKEY_VALUE, 0, &mkeys, LED_TIMEOUT) {
            log::warn!("mkey LED write failed: {e}");
            return;
        }
        *last = Some(want);
    }
}
