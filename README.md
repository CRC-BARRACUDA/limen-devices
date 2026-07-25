# devices

A native [Limen](https://github.com/CRC-BARRACUDA/Limen) module that lists the
devices on **this** machine across several buses — with USB history where the OS
keeps it.

Provides the capability **`devices.local`**.

## What it does

Enumerates devices in six **categories** and presents them in one searchable,
refreshable table (split into **Connected** now vs **Disconnected**/previously
seen):

| Category | What |
|---|---|
| `usb` | USB devices, classified by type (flash, keyboard, mouse, camera, audio, smartcard, hub, …) |
| `pci` | PCI/PCIe functions (GPU, network, storage/serial-bus controllers, …) |
| `monitor` | Attached displays — manufacturer, model, serial from **EDID** |
| `disk` | Non-USB drives (NVMe/SATA), with model + serial |
| `net` | Network interfaces (ethernet / wifi / virtual), MAC, link state |
| `bluetooth` | Bluetooth adapters |

Per-OS implementation (`cfg`-selected at compile time):

| OS | Source |
|---|---|
| **Linux** | `/sys` — `bus/usb`, `bus/pci`, `class/drm` (EDID), `block`, `class/net`, `class/bluetooth` — plus `journalctl -k` for USB history |
| **Windows** | Registry `…\Enum\{USB, USBSTOR, PCI, DISPLAY, SCSI, IDE, BTHENUM}` (devices ever connected) |

## Methods

| Method | Returns |
|---|---|
| `list` | JSON: `{os, total, connected, was_connected, devices[]}` — each device has `category`, `type`, `id`, `vendor`, `product`, `serial`, `connected`. Always scans |
| `ui`   | The landing view — a **Scan** button. Nothing is enumerated until pressed |
| `scan` | Runs the scan and returns the results view (Connected/Disconnected tables + search + Refresh) |

## Permissions

```toml
[permissions]
subprocess = true    # Linux: spawns `journalctl` to read device history
```

## Install

```bash
limen-cli add CRC-BARRACUDA/limen-devices@0.2.0
```

Limen clones the source, reads `limen.toml`, sees `language = "native"`, and
downloads the prebuilt library for your platform from the release assets (saved
locally as `libdevices.so`). No build step on install.

## Build from source

It's a `cdylib` built against `limen-sdk-rust`:

```bash
cargo build --release        # → target/release/libdevices.so
```

## Releasing (for maintainers)

Limen's package manager picks the release asset whose name **ends with** the
platform extension (`.so` / `.dll` / `.dylib`) **and contains** the arch token
(`x86_64` / `aarch64`). Name assets accordingly:

| Platform | Asset name |
|---|---|
| Linux x64 | `devices-linux-x86_64.so` |
| Windows x64 | `devices-windows-x86_64.dll` |
| macOS ARM | `devices-macos-aarch64.dylib` |

Tag the release with the module version so `add …@<version>` resolves it:

```bash
cargo build --release
cp target/release/libdevices.so devices-linux-x86_64.so
strip devices-linux-x86_64.so

gh release create 0.2.0 \
  --repo CRC-BARRACUDA/limen-devices \
  --title "devices 0.2.0" \
  devices-linux-x86_64.so
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
