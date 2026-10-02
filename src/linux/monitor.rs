//! Attached displays from `/sys/class/drm/*/edid` (manufacturer / model / serial
//! decoded from the EDID block) with live connect state from the `status` file.

use limen_sdk_rust::Value;

use super::read_trim;
use crate::device;

pub(super) fn collect(out: &mut Vec<Value>) {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        let connected = read_trim(&dir.join("status")).as_deref() == Some("connected");
        let Ok(edid) = std::fs::read(dir.join("edid")) else {
            continue;
        };
        if edid.len() < 128 {
            continue; // no attached display on this connector
        }
        let (vendor, product, serial) = parse_edid(&edid);
        let connector = entry.file_name().to_string_lossy().to_string();
        let path = Some(dir.to_string_lossy().into_owned());
        out.push(device("monitor", "display", connector, vendor, product, serial, connected, path));
    }
}

/// Extract (manufacturer PNP id, model name, serial) from a 128-byte EDID block.
fn parse_edid(e: &[u8]) -> (Option<String>, Option<String>, Option<String>) {
    // Manufacturer: 3 letters packed in bytes 8-9 (5 bits each, 'A'=1).
    let m = ((e[8] as u16) << 8) | e[9] as u16;
    let letter = |v: u16| (b'A' - 1 + (v as u8)) as char;
    let vendor = (m != 0).then(|| {
        [letter((m >> 10) & 0x1f), letter((m >> 5) & 0x1f), letter(m & 0x1f)]
            .iter()
            .collect::<String>()
    });

    // Descriptor blocks at 54/72/90/108; tag 0xFC = name, 0xFF = serial string.
    let mut name = None;
    let mut serial = None;
    for base in [54usize, 72, 90, 108] {
        if base + 18 > e.len() {
            break;
        }
        let d = &e[base..base + 18];
        if d[0] == 0 && d[1] == 0 && d[2] == 0 {
            let text = || {
                let s: String = d[5..]
                    .iter()
                    .take_while(|&&c| c != 0x0a)
                    .map(|&c| c as char)
                    .collect();
                let s = s.trim().to_string();
                (!s.is_empty()).then_some(s)
            };
            match d[3] {
                0xFC => name = text(),
                0xFF => serial = text(),
                _ => {}
            }
        }
    }
    (vendor, name, serial)
}
