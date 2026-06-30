use anyhow::{Context, Result};
use rusb::{DeviceHandle, GlobalContext};
use std::sync::mpsc::Sender;
use std::time::Duration;
use crate::protocol::{G13Event, ReportParser};

const G13_VID: u16 = 0x046D;
const G13_PID: u16 = 0xC21C;
const ENDPOINT_IN: u8 = 0x81;
const READ_TIMEOUT: Duration = Duration::from_millis(100);

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

    pub fn run(mut self, tx: Sender<G13Event>) -> Result<()> {
        let mut parser = ReportParser::new();
        let mut buf = [0u8; 8];
        loop {
            match self.handle.read_interrupt(ENDPOINT_IN, &mut buf, READ_TIMEOUT) {
                Ok(8) => {
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
}
