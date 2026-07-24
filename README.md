# local-devices

A native [Limen](https://github.com/CRC-BARRACUDA/Limen) module that lists the
devices connected to **this** machine — with USB history where the OS keeps it.

Provides the capability **`devices.local`**.

## What it does

- Enumerates USB devices and classifies each by **type** — flash drive, keyboard,
  mouse, camera, audio, smartcard, hub, etc. (derived from the USB interface
  class/protocol).
- Splits them into **Connected** (attached now) and **Disconnected** (seen before
  but not currently attached).
- A searchable, refreshable table UI drawn by the module itself.

Per-OS implementation (`cfg`-selected at compile time):

| OS | Source | History? |
|---|---|---|
| **Windows** | Registry `…\Enum\USB` + `USBSTOR` | Yes — *every* device ever connected (the classic "USB history") |
| **Linux** | `/sys/bus/usb/devices/` + the kernel journal (`journalctl -k`) | Currently attached, plus what the journal remembers |

## Methods

| Method | Returns |
|---|---|
| `list` | JSON array of devices (`vendor`, `product`, `vid`/`pid`, `serial`, `type`, `connected`) |
| `ui`   | A Limen view spec — the Connected/Disconnected tables with search + Refresh |

## Permissions

```toml
[permissions]
subprocess = true    # Linux: spawns `journalctl` to read device history
```

## Install

```bash
limen-cli add CRC-BARRACUDA/limen-local-devices@0.1.0
```

Limen clones the source, reads `limen.toml`, sees `language = "native"`, and
downloads the prebuilt library for your platform from the release assets (saved
locally as `liblocal_devices.so`). No build step on install.

## Build from source

It's a `cdylib` built against `limen-sdk-rust`:

```bash
cargo build --release        # → target/release/liblocal_devices.so
```

## Releasing (for maintainers)

Limen's package manager picks the release asset whose name **ends with** the
platform extension (`.so` / `.dll` / `.dylib`) **and contains** the arch token
(`x86_64` / `aarch64`). Name assets accordingly:

| Platform | Asset name |
|---|---|
| Linux x64 | `local_devices-linux-x86_64.so` |
| Windows x64 | `local_devices-windows-x86_64.dll` |
| macOS ARM | `local_devices-macos-aarch64.dylib` |

Tag the release with the module version so `add …@<version>` resolves it:

```bash
cargo build --release
cp target/release/liblocal_devices.so local_devices-linux-x86_64.so
strip local_devices-linux-x86_64.so

gh release create 0.1.0 \
  --repo CRC-BARRACUDA/limen-local-devices \
  --title "local-devices 0.1.0" \
  local_devices-linux-x86_64.so
```

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
