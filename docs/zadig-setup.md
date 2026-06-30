# Installing WinUSB for the G13 (one-time setup)

The G13 driver reads USB data directly via libusb. On Windows, this requires
replacing the default HID driver with WinUSB using Zadig. This is a one-time
step per machine.

## Steps

1. **Download Zadig** from https://zadig.akeo.ie/ — no installation needed.

2. **Plug in the G13** via USB.

3. **Run Zadig** as Administrator.

4. In the menu bar, click **Options → List All Devices**.

5. In the device dropdown, find **Logitech G13** (or a device with
   VID `046D`, PID `C21C`). If multiple G13 entries appear, select the
   one labelled "Interface 0".

6. In the driver box on the right, select **WinUSB**.

7. Click **Replace Driver** and wait for it to finish.

8. The G13 is now accessible to g13-driver. You only need to repeat this
   if you reinstall Windows or connect the G13 to a different physical USB
   port for the first time.

## Reverting

To restore the original HID driver (e.g., to use Logitech GHub again):
open Device Manager, find the G13 under "Universal Serial Bus devices",
right-click → **Update driver** → **Browse my computer** → **Let me pick**
→ select **HID-compliant game controller**.
