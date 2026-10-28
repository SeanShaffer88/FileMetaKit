//! Hand-written, bounds-checked metadata parsers (zero dependencies).
//! Every parser takes a byte slice and fills `Meta`; a truncated or
//! corrupt file sets `parse_error` instead of panicking.

use crate::{json_escape, ERR_INVALID_ARG, ERR_NOT_FOUND, ERR_NOT_SUPPORTED, ERR_PLATFORM};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub enum FVal {
    S(String),
    I(i64),
    U(u64),
    F(f64),
    B(bool),
}

impl FVal {
    pub fn to_json(&self) -> String {
        match self {
            FVal::S(s) => format!("\"{}\"", json_escape(s)),
            FVal::I(n) => n.to_string(),
            FVal::U(n) => n.to_string(),
            FVal::F(f) => {
                if f.is_finite() {
                    let s = format!("{f:.6}");
                    s.trim_end_matches('0').trim_end_matches('.').to_string()
                } else {
                    "null".into()
                }
            }
            FVal::B(b) => b.to_string(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Meta {
    pub file_name: String,
    pub extension: String,
    pub size: u64,
    pub modified_unix: u64,
    pub kind: &'static str,
    pub mime: &'static str,
    pub supported: bool,
    pub reason: Option<String>,
    pub parse_error: Option<String>,
    /// Insertion-ordered fields; duplicate keys are overwritten.
    pub fields: Vec<(String, FVal)>,
}

impl Meta {
    pub fn set(&mut self, key: &str, val: FVal) {
        if let Some(slot) = self.fields.iter_mut().find(|(k, _)| k == key) {
            slot.1 = val;
        } else {
            self.fields.push((key.to_string(), val));
        }
    }
    pub fn get(&self, key: &str) -> Option<&FVal> {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn to_json(&self) -> String {
        let mut s = String::new();
        s.push_str("{\"file_name\":\"");
        s.push_str(&json_escape(&self.file_name));
        s.push_str("\",\"extension\":\"");
        s.push_str(&json_escape(&self.extension));
        s.push_str(&format!(
            "\",\"size\":{},\"modified_unix\":{},\"kind\":\"{}\",\"mime\":\"{}\",\"supported\":{}",
            self.size, self.modified_unix, self.kind, self.mime, self.supported
        ));
        if let Some(r) = &self.reason {
            s.push_str(&format!(",\"reason\":\"{}\"", json_escape(r)));
        }
        if let Some(e) = &self.parse_error {
            s.push_str(&format!(",\"parse_error\":\"{}\"", json_escape(e)));
        }
        s.push_str(",\"fields\":{");
        for (i, (k, v)) in self.fields.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!("\"{}\":{}", json_escape(k), v.to_json()));
        }
        s.push_str("}}");
        s
    }
}

fn kind_mime(ext: &str) -> (&'static str, &'static str, bool) {
    match ext {
        "png" => ("image", "image/png", true),
        "jpg" | "jpeg" => ("image", "image/jpeg", true),
        "gif" => ("image", "image/gif", true),
        "bmp" => ("image", "image/bmp", true),
        "webp" => ("image", "image/webp", true),
        "wav" => ("audio", "audio/wav", true),
        "mp3" => ("audio", "audio/mpeg", true),
        "flac" => ("audio", "audio/flac", true),
        "ogg" | "oga" => ("audio", "audio/ogg", true),
        "mp4" | "m4v" => ("video", "video/mp4", true),
        "mov" => ("video", "video/quicktime", true),
        "mkv" => ("video", "video/x-matroska", true),
        "webm" => ("video", "video/webm", true),
        "pdf" => ("document", "application/pdf", true),
        "txt" => ("document", "text/plain", true),
        "md" => ("document", "text/markdown", true),
        _ => ("other", "application/octet-stream", false),
    }
}

// ---------- bounds-checked readers ----------
pub fn u16le(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
pub fn u16be(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|s| u16::from_be_bytes([s[0], s[1]]))
}
pub fn u32le(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
pub fn u32be(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}
pub fn u64be(d: &[u8], o: usize) -> Option<u64> {
    d.get(o..o + 8)
        .map(|s| u64::from_be_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
}
pub fn i32le(d: &[u8], o: usize) -> Option<i32> {
    u32le(d, o).map(|v| v as i32)
}

/// CRC-32 (IEEE, as used by PNG) — table built at compile time.
pub const fn make_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}
pub static CRC_TABLE: [u32; 256] = make_crc_table();
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}
fn trim_nul(s: &str) -> String {
    s.trim_end_matches('\0').trim().to_string()
}

// ---------- entry points ----------

pub fn read_metadata_bytes(file_name: &str, data: &[u8], modified_unix: u64) -> Meta {
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (kind, mime, supported) = kind_mime(&ext);
    let mut m = Meta {
        file_name: Path::new(file_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(file_name)
            .to_string(),
        extension: ext.clone(),
        size: data.len() as u64,
        modified_unix,
        kind,
        mime,
        supported,
        ..Default::default()
    };
    if !supported {
        m.reason = Some(format!("unsupported extension \"{ext}\""));
        return m;
    }
    let r: Result<(), String> = match ext.as_str() {
        "png" => parse_png(data, &mut m),
        "jpg" | "jpeg" => parse_jpeg(data, &mut m),
        "gif" => parse_gif(data, &mut m),
        "bmp" => parse_bmp(data, &mut m),
        "webp" => parse_webp(data, &mut m),
        "wav" => parse_wav(data, &mut m),
        "mp3" => parse_mp3(data, &mut m),
        "flac" => parse_flac(data, &mut m),
        "ogg" | "oga" => parse_ogg(data, &mut m),
        "mp4" | "m4v" | "mov" => parse_mp4(data, &mut m),
        "mkv" | "webm" => parse_mkv(data, &mut m),
        "pdf" => parse_pdf(data, &mut m),
        "txt" | "md" => parse_text(data, &mut m),
        _ => Ok(()),
    };
    if let Err(e) = r {
        m.parse_error = Some(e);
    }
    m
}

pub type MResult<T> = Result<T, (i32, String)>;

fn load_file(path: &str) -> MResult<(Vec<u8>, u64)> {
    let p = Path::new(path);
    let bytes = std::fs::read(p).map_err(|e| {
        let code = if e.kind() == std::io::ErrorKind::NotFound {
            ERR_NOT_FOUND
        } else if e.kind() == std::io::ErrorKind::PermissionDenied {
            crate::ERR_ACCESS_DENIED
        } else {
            ERR_PLATFORM
        };
        (code, format!("cannot read \"{path}\": {e}"))
    })?;
    let modified = std::fs::metadata(p)
        .and_then(|md| md.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok((bytes, modified))
}

pub fn read_file_json(path: &str) -> MResult<String> {
    if path.is_empty() {
        return Err((ERR_INVALID_ARG, "path is empty".into()));
    }
    let (bytes, modified) = load_file(path)?;
    Ok(read_metadata_bytes(path, &bytes, modified).to_json())
}

pub fn compare_file_json(path_a: &str, path_b: &str) -> MResult<String> {
    let (ba, ma) = load_file(path_a)?;
    let (bb, mb) = load_file(path_b)?;
    let a = read_metadata_bytes(path_a, &ba, ma);
    let b = read_metadata_bytes(path_b, &bb, mb);
    Ok(compare_meta(&a, &b))
}

pub fn compare_meta(a: &Meta, b: &Meta) -> String {
    let same_format = a.extension == b.extension;
    let mut keys: Vec<&str> = Vec::new();
    for (k, _) in a.fields.iter().chain(b.fields.iter()) {
        if !keys.contains(&k.as_str()) {
            keys.push(k.as_str());
        }
    }
    keys.sort_unstable();
    let mut diffs = String::new();
    let mut n_diff = 0usize;
    for k in keys {
        let va = a.get(k);
        let vb = b.get(k);
        if va != vb {
            if n_diff > 0 {
                diffs.push(',');
            }
            n_diff += 1;
            let ja = va.map(|v| v.to_json()).unwrap_or_else(|| "null".into());
            let jb = vb.map(|v| v.to_json()).unwrap_or_else(|| "null".into());
            diffs.push_str(&format!(
                "{{\"field\":\"{}\",\"a\":{},\"b\":{}}}",
                json_escape(k),
                ja,
                jb
            ));
        }
    }
    format!(
        "{{\"same_format\":{},\"differences\":[{}],\"identical_metadata\":{}}}",
        same_format,
        diffs,
        n_diff == 0
    )
}

// ---------- PNG ----------
fn parse_png(d: &[u8], m: &mut Meta) -> Result<(), String> {
    if d.len() < 8 || &d[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err("not a PNG (bad signature)".into());
    }
    let mut pos = 8usize;
    let mut saw_ihdr = false;
    while pos + 8 <= d.len() {
        let len = u32be(d, pos).ok_or("truncated chunk header")? as usize;
        let ctype = &d[pos + 4..pos + 8];
        let data_start = pos + 8;
        let data_end = data_start
            .checked_add(len)
            .filter(|&e| e + 4 <= d.len())
            .ok_or("truncated chunk data")?;
        let chunk = &d[data_start..data_end];
        match ctype {
            b"IHDR" => {
                if chunk.len() < 13 {
                    return Err("IHDR too short".into());
                }
                saw_ihdr = true;
                m.set("width", FVal::U(u32be(chunk, 0).unwrap_or(0) as u64));
                m.set("height", FVal::U(u32be(chunk, 4).unwrap_or(0) as u64));
                m.set("bit_depth", FVal::U(chunk[8] as u64));
                m.set("color_type", FVal::U(chunk[9] as u64));
            }
            b"tEXt" => {
                if let Some(z) = chunk.iter().position(|&b| b == 0) {
                    let key = trim_nul(&latin1(&chunk[..z]));
                    let val = latin1(&chunk[z + 1..]);
                    if !key.is_empty() {
                        m.set(&key, FVal::S(val));
                    }
                }
            }
            b"IEND" => break,
            _ => {}
        }
        pos = data_end + 4;
    }
    if !saw_ihdr {
        return Err("PNG has no IHDR chunk".into());
    }
    if pos > d.len() {
        return Err("PNG truncated".into());
    }
    Ok(())
}

// ---------- JPEG / EXIF ----------
fn parse_jpeg(d: &[u8], m: &mut Meta) -> Result<(), String> {
    if d.len() < 2 || d[0] != 0xFF || d[1] != 0xD8 {
        return Err("not a JPEG (missing SOI)".into());
    }
    let mut pos = 2usize;
    let mut saw_sof = false;
    while pos + 1 < d.len() {
        if d[pos] != 0xFF {
            return Err(format!("JPEG marker expected at byte {pos}"));
        }
        let mut marker = d[pos + 1];
        let mut hdr = pos + 2;
        while marker == 0xFF && hdr < d.len() {
            marker = d[hdr];
            hdr += 1;
        }
        if marker == 0xD9 || marker == 0xDA {
            break; // EOI / start of scan
        }
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            pos = hdr;
            continue;
        }
        let seglen = u16be(d, hdr).ok_or("truncated JPEG segment")? as usize;
        if seglen < 2 || hdr + seglen > d.len() {
            return Err("truncated JPEG segment".into());
        }
        let seg = &d[hdr + 2..hdr + seglen];
        match marker {
            0xC0..=0xC2 => {
                if seg.len() >= 5 {
                    saw_sof = true;
                    m.set("height", FVal::U(u16be(seg, 1).unwrap_or(0) as u64));
                    m.set("width", FVal::U(u16be(seg, 3).unwrap_or(0) as u64));
                }
            }
            0xE1 if seg.starts_with(b"Exif\0\0") => {
                parse_exif(&seg[6..], m);
            }
            _ => {}
        }
        pos = hdr + seglen;
    }
    if !saw_sof {
        return Err("JPEG has no SOF segment".into());
    }
    Ok(())
}

fn tiff_u16(d: &[u8], o: usize, le: bool) -> Option<u16> {
    if le {
        u16le(d, o)
    } else {
        u16be(d, o)
    }
}
fn tiff_u32(d: &[u8], o: usize, le: bool) -> Option<u32> {
    if le {
        u32le(d, o)
    } else {
        u32be(d, o)
    }
}
fn type_size(t: u16) -> usize {
    match t {
        1 | 2 | 7 => 1,
        3 => 2,
        4 | 9 => 4,
        5 | 10 => 8,
        _ => 1,
    }
}
fn parse_exif(t: &[u8], m: &mut Meta) {
    // Best-effort sub-parser: any inconsistency just yields fewer fields.
    let le = match t.get(0..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return,
    };
    if tiff_u16(t, 2, le) != Some(42) {
        return;
    }
    let ifd0 = match tiff_u32(t, 4, le) {
        Some(v) => v as usize,
        None => return,
    };
    let count = match t.get(ifd0..ifd0 + 2).map(|s| {
        if le {
            u16le(s, 0).unwrap_or(0)
        } else {
            u16be(s, 0).unwrap_or(0)
        }
    }) {
        Some(c) => c as usize,
        None => return,
    };
    for i in 0..count {
        let e = ifd0 + 2 + i * 12;
        let entry = match t.get(e..e + 12) {
            Some(s) => s,
            None => return,
        };
        let tag = tiff_u16(entry, 0, le).unwrap_or(0);
        let typ = tiff_u16(entry, 2, le).unwrap_or(0);
        let n = tiff_u32(entry, 4, le).unwrap_or(0) as usize;
        let total = n.saturating_mul(type_size(typ));
        let val_bytes: &[u8] = if total <= 4 {
            &entry[8..8 + total]
        } else {
            let off = tiff_u32(entry, 8, le).unwrap_or(0) as usize;
            match t.get(off..off + total) {
                Some(s) => s,
                None => continue,
            }
        };
        match tag {
            0x010F | 0x0110 | 0x0132 => {
                let key = match tag {
                    0x010F => "exif_make",
                    0x0110 => "exif_model",
                    _ => "exif_datetime",
                };
                m.set(key, FVal::S(trim_nul(&latin1(val_bytes))));
            }
            0x0112 => {
                if let Some(v) = tiff_u16(val_bytes, 0, le) {
                    m.set("exif_orientation", FVal::U(v as u64));
                }
            }
            _ => {}
        }
    }
}

// ---------- GIF ----------
fn gif_skip_subblocks(d: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        let n = *d.get(pos)? as usize;
        pos += 1;
        if n == 0 {
            return Some(pos);
        }
        pos = pos.checked_add(n).filter(|&p| p <= d.len())?;
    }
}
fn parse_gif(d: &[u8], m: &mut Meta) -> Result<(), String> {
    if d.len() < 13 || !(d.starts_with(b"GIF87a") || d.starts_with(b"GIF89a")) {
        return Err("not a GIF (bad header)".into());
    }
    m.set("width", FVal::U(u16le(d, 6).unwrap_or(0) as u64));
    m.set("height", FVal::U(u16le(d, 8).unwrap_or(0) as u64));
    let packed = d[10];
    let mut pos = 13usize;
    if packed & 0x80 != 0 {
        let gct = 3usize << ((packed & 0x07) + 1);
        pos += gct;
        if pos > d.len() {
            return Err("GIF truncated in global colour table".into());
        }
    }
    let mut frames: u64 = 0;
    loop {
        match d.get(pos) {
            None => break,
            Some(0x3B) => break,
            Some(0x21) => {
                // extension: label byte then sub-blocks
                match gif_skip_subblocks(d, pos + 2) {
                    Some(p) => pos = p,
                    None => return Err("GIF truncated in extension".into()),
                }
            }
            Some(0x2C) => {
                frames += 1;
                // descriptor: 9 bytes from separator, then optional LCT
                let desc_packed = match d.get(pos + 9) {
                    Some(v) => *v,
                    None => return Err("GIF truncated in image descriptor".into()),
                };
                let mut p = pos + 10;
                if desc_packed & 0x80 != 0 {
                    p += 3usize << ((desc_packed & 0x07) + 1);
                }
                p += 1; // LZW minimum code size
                match gif_skip_subblocks(d, p) {
                    Some(np) => pos = np,
                    None => return Err("GIF truncated in image data".into()),
                }
            }
            Some(other) => {
                return Err(format!("GIF: unknown block byte 0x{other:02x}"));
            }
        }
    }
    m.set("frames", FVal::U(frames));
    m.set("animated", FVal::B(frames > 1));
    Ok(())
}

// ---------- BMP ----------
fn parse_bmp(d: &[u8], m: &mut Meta) -> Result<(), String> {
    if d.len() < 26 || !d.starts_with(b"BM") {
        return Err("not a BMP (missing BM)".into());
    }
    let dib = u32le(d, 14).ok_or("BMP truncated")?;
    if dib >= 40 {
        let w = i32le(d, 18).unwrap_or(0);
        let h = i32le(d, 22).unwrap_or(0);
        m.set("width", FVal::I(w as i64));
        m.set("height", FVal::I((h as i64).abs()));
        m.set("bpp", FVal::U(u16le(d, 28).unwrap_or(0) as u64));
    } else if dib == 12 {
        m.set("width", FVal::U(u16le(d, 18).unwrap_or(0) as u64));
        m.set("height", FVal::U(u16le(d, 20).unwrap_or(0) as u64));
        m.set("bpp", FVal::U(u16le(d, 24).unwrap_or(0) as u64));
    } else {
        return Err(format!("unsupported BMP DIB header size {dib}"));
    }
    Ok(())
}

// ---------- WebP ----------
fn parse_webp(d: &[u8], m: &mut Meta) -> Result<(), String> {
    if d.len() < 20 || !d.starts_with(b"RIFF") || &d[8..12] != b"WEBP" {
        return Err("not a WebP (bad RIFF header)".into());
    }
    let mut pos = 12usize;
    while pos + 8 <= d.len() {
        let fourcc = &d[pos..pos + 4];
        let size = u32le(d, pos + 4).unwrap_or(0) as usize;
        let data = d
            .get(pos + 8..pos + 8 + size)
            .ok_or("WebP truncated chunk")?;
        match fourcc {
            b"VP8X" => {
                if data.len() < 10 {
                    return Err("VP8X too short".into());
                }
                let w = (data[4] as u32) | ((data[5] as u32) << 8) | ((data[6] as u32) << 16);
                let h = (data[7] as u32) | ((data[8] as u32) << 8) | ((data[9] as u32) << 16);
                m.set("width", FVal::U((w + 1) as u64));
                m.set("height", FVal::U((h + 1) as u64));
            }
            b"VP8L" => {
                if data.len() < 5 || data[0] != 0x2F {
                    return Err("VP8L too short / bad signature".into());
                }
                let bits = u32le(data, 1).unwrap_or(0);
                m.set("width", FVal::U(((bits & 0x3FFF) + 1) as u64));
                m.set("height", FVal::U((((bits >> 14) & 0x3FFF) + 1) as u64));
            }
            b"VP8 " => {
                if data.len() < 10 || &data[3..6] != b"\x9d\x01\x2a" {
                    return Err("VP8 frame header not found".into());
                }
                let w = u16le(data, 6).unwrap_or(0) & 0x3FFF;
                let h = u16le(data, 8).unwrap_or(0) & 0x3FFF;
                m.set("width", FVal::U(w as u64));
                m.set("height", FVal::U(h as u64));
            }
            _ => {}
        }
        if m.get("width").is_some() {
            return Ok(());
        }
        pos += 8 + size + (size % 2);
    }
    Err("WebP: no VP8/VP8L/VP8X chunk found".into())
}

// ---------- WAV ----------
fn parse_wav(d: &[u8], m: &mut Meta) -> Result<(), String> {
    if d.len() < 12 || !d.starts_with(b"RIFF") || &d[8..12] != b"WAVE" {
        return Err("not a WAV (bad RIFF header)".into());
    }
    let mut pos = 12usize;
    let mut byte_rate: u32 = 0;
    let mut data_size: u64 = 0;
    let mut saw_fmt = false;
    while pos + 8 <= d.len() {
        let id = &d[pos..pos + 4];
        let size = u32le(d, pos + 4).unwrap_or(0) as usize;
        let chunk = match d.get(pos + 8..pos + 8 + size) {
            Some(c) => c,
            None => return Err("WAV truncated chunk".into()),
        };
        match id {
            b"fmt " => {
                if chunk.len() < 16 {
                    return Err("WAV fmt chunk too short".into());
                }
                saw_fmt = true;
                m.set("channels", FVal::U(u16le(chunk, 2).unwrap_or(0) as u64));
                m.set("sample_rate", FVal::U(u32le(chunk, 4).unwrap_or(0) as u64));
                byte_rate = u32le(chunk, 8).unwrap_or(0);
                m.set("bits", FVal::U(u16le(chunk, 14).unwrap_or(0) as u64));
            }
            b"data" => data_size = size as u64,
            _ => {}
        }
        pos += 8 + size + (size % 2);
    }
    if !saw_fmt {
        return Err("WAV has no fmt chunk".into());
    }
    if byte_rate > 0 {
        m.set("duration_sec", FVal::F(data_size as f64 / byte_rate as f64));
    }
    Ok(())
}

// ---------- shared tag helpers ----------
fn parse_vorbis_comment(d: &[u8], m: &mut Meta) {
    // d starts at the vendor-length field.
    let mut pos = 0usize;
    let vendor_len = match u32le(d, pos) {
        Some(v) => v as usize,
        None => return,
    };
    pos += 4 + vendor_len;
    let count = match u32le(d, pos) {
        Some(v) => v as usize,
        None => return,
    };
    pos += 4;
    for _ in 0..count {
        let len = match u32le(d, pos) {
            Some(v) => v as usize,
            None => return,
        };
        pos += 4;
        let entry = match d.get(pos..pos + len) {
            Some(s) => s,
            None => return,
        };
        pos += len;
        if let Ok(text) = std::str::from_utf8(entry) {
            if let Some(eq) = text.find('=') {
                let key = text[..eq].to_ascii_lowercase();
                let val = text[eq + 1..].to_string();
                let mapped = match key.as_str() {
                    "title" => Some("title"),
                    "artist" => Some("artist"),
                    "album" => Some("album"),
                    "date" => Some("year"),
                    "tracknumber" => Some("track"),
                    "genre" => Some("genre"),
                    _ => None,
                };
                if let Some(k) = mapped {
                    m.set(k, FVal::S(val));
                }
            }
        }
    }
}

pub const GENRES: &[&str] = &[
    "Blues",
    "Classic Rock",
    "Country",
    "Dance",
    "Disco",
    "Funk",
    "Grunge",
    "Hip-Hop",
    "Jazz",
    "Metal",
    "New Age",
    "Oldies",
    "Other",
    "Pop",
    "R&B",
    "Rap",
    "Reggae",
    "Rock",
    "Techno",
    "Industrial",
    "Alternative",
    "Ska",
    "Death Metal",
    "Pranks",
    "Soundtrack",
    "Euro-Techno",
    "Ambient",
    "Trip-Hop",
    "Vocal",
    "Jazz+Funk",
];

// ---------- MP3 ----------
fn decode_id3_text(raw: &[u8]) -> String {
    if raw.is_empty() {
        return String::new();
    }
    match raw[0] {
        3 => trim_nul(&String::from_utf8_lossy(&raw[1..])),
        1 => {
            // UTF-16 with BOM
            let body = &raw[1..];
            let le = body.starts_with(&[0xFF, 0xFE]);
            let be = body.starts_with(&[0xFE, 0xFF]);
            let body = if le || be { &body[2..] } else { body };
            let units: Vec<u16> = body
                .chunks_exact(2)
                .map(|c| {
                    if be {
                        u16::from_be_bytes([c[0], c[1]])
                    } else {
                        u16::from_le_bytes([c[0], c[1]])
                    }
                })
                .collect();
            trim_nul(&String::from_utf16_lossy(&units))
        }
        2 => {
            let units: Vec<u16> = raw[1..]
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            trim_nul(&String::from_utf16_lossy(&units))
        }
        _ => trim_nul(&latin1(&raw[1..])),
    }
}

fn parse_id3v2(d: &[u8], m: &mut Meta) -> usize {
    // Returns total tag size (header + body + optional v2.4 footer), 0 if none.
    if d.len() < 10 || !d.starts_with(b"ID3") {
        return 0;
    }
    let major = d[3];
    let size = (((d[6] & 0x7F) as usize) << 21)
        | (((d[7] & 0x7F) as usize) << 14)
        | (((d[8] & 0x7F) as usize) << 7)
        | ((d[9] & 0x7F) as usize);
    let body_end = 10usize.saturating_add(size).min(d.len());
    let mut pos = 10usize;
    while pos + 10 <= body_end {
        let id = &d[pos..pos + 4];
        if !id.iter().all(|b| b.is_ascii_alphanumeric()) {
            break;
        }
        let fsize = if major >= 4 {
            (((d[pos + 4] & 0x7F) as usize) << 21)
                | (((d[pos + 5] & 0x7F) as usize) << 14)
                | (((d[pos + 6] & 0x7F) as usize) << 7)
                | ((d[pos + 7] & 0x7F) as usize)
        } else {
            u32be(d, pos + 4).unwrap_or(0) as usize
        };
        let start = pos + 10;
        let end = match start.checked_add(fsize).filter(|&e| e <= body_end) {
            Some(e) => e,
            None => break,
        };
        let frame = &d[start..end];
        let key = match id {
            b"TIT2" => Some("title"),
            b"TPE1" => Some("artist"),
            b"TALB" => Some("album"),
            b"TYER" | b"TDRC" => Some("year"),
            b"TRCK" => Some("track"),
            b"TCON" => Some("genre"),
            _ => None,
        };
        if let Some(k) = key {
            let text = decode_id3_text(frame);
            let text = if k == "year" {
                text.chars().take(4).collect()
            } else {
                text
            };
            if !text.is_empty() {
                m.set(k, FVal::S(text));
            }
        }
        pos = end;
    }
    let footer = if major >= 4 && d[5] & 0x10 != 0 {
        10
    } else {
        0
    };
    10 + size + footer
}

fn id3v1_fields(d: &[u8]) -> Vec<(String, FVal)> {
    let mut out = Vec::new();
    if d.len() < 128 {
        return out;
    }
    let tag = &d[d.len() - 128..];
    if &tag[0..3] != b"TAG" {
        return out;
    }
    let field = |a: usize, b: usize| trim_nul(&latin1(&tag[a..b]));
    for (key, a, b) in [
        ("title", 3usize, 33usize),
        ("artist", 33, 63),
        ("album", 63, 93),
        ("year", 93, 97),
    ] {
        let v = field(a, b);
        if !v.is_empty() {
            out.push((key.to_string(), FVal::S(v)));
        }
    }
    // Track: comment is 28 bytes + zero + track byte.
    if tag[125] == 0 && tag[126] != 0 {
        let c = field(97, 125);
        if !c.is_empty() {
            out.push(("comment".into(), FVal::S(c)));
        }
        out.push(("track".into(), FVal::S((tag[126]).to_string())));
    } else {
        let c = field(97, 127);
        if !c.is_empty() {
            out.push(("comment".into(), FVal::S(c)));
        }
    }
    let g = tag[127] as usize;
    if let Some(name) = GENRES.get(g) {
        out.push(("genre".into(), FVal::S((*name).to_string())));
    }
    out
}

fn parse_mp3(d: &[u8], m: &mut Meta) -> Result<(), String> {
    let v2_size = parse_id3v2(d, m);
    for (k, v) in id3v1_fields(d) {
        if m.get(&k).is_none() {
            m.set(&k, v);
        }
    }
    // First MPEG audio frame: search a small window from the v2 end.
    let start = v2_size.min(d.len());
    let scan_end = (start + 1024).min(d.len().saturating_sub(4));
    let mut frame_off: Option<usize> = None;
    let mut i = start;
    while i < scan_end {
        if d[i] == 0xFF && d[i + 1] & 0xE0 == 0xE0 {
            frame_off = Some(i);
            break;
        }
        i += 1;
    }
    let off = match frame_off {
        Some(o) => o,
        None => {
            if m.fields.is_empty() {
                return Err("MP3: no ID3 tag and no MPEG frame found".into());
            }
            return Ok(());
        }
    };
    let h = &d[off..off + 4];
    let version_bits = (h[1] >> 3) & 0x03; // 3=MPEG1, 2=MPEG2, 0=MPEG2.5
    let layer_bits = (h[1] >> 1) & 0x03; // 1 = Layer III
    let br_idx = ((h[2] >> 4) & 0x0F) as usize;
    let sr_idx = ((h[2] >> 2) & 0x03) as usize;
    if layer_bits == 1 && br_idx > 0 && br_idx < 15 && sr_idx < 3 {
        let br_table_v1: [u32; 15] = [
            0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
        ];
        let br_table_v2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
        let kbps = if version_bits == 3 {
            br_table_v1[br_idx]
        } else {
            br_table_v2[br_idx]
        };
        let base_sr: [u32; 3] = [44100, 48000, 32000];
        let sr = match version_bits {
            3 => base_sr[sr_idx],
            2 => base_sr[sr_idx] / 2,
            0 => base_sr[sr_idx] / 4,
            _ => base_sr[sr_idx],
        };
        if kbps > 0 {
            m.set("bitrate_kbps", FVal::U(kbps as u64));
            m.set("sample_rate", FVal::U(sr as u64));
            let audio_bytes = d.len().saturating_sub(v2_size) as f64;
            // Exclude a trailing ID3v1 tag from the estimate.
            let audio_bytes = if d.len() >= 128 && &d[d.len() - 128..d.len() - 125] == b"TAG" {
                audio_bytes - 128.0
            } else {
                audio_bytes
            };
            m.set(
                "duration_sec",
                FVal::F(audio_bytes * 8.0 / (kbps as f64 * 1000.0)),
            );
        }
    }
    Ok(())
}

// ---------- FLAC ----------
fn parse_flac(d: &[u8], m: &mut Meta) -> Result<(), String> {
    if d.len() < 8 || !d.starts_with(b"fLaC") {
        return Err("not a FLAC (missing fLaC)".into());
    }
    let mut pos = 4usize;
    loop {
        if pos + 4 > d.len() {
            return Err("FLAC truncated in metadata blocks".into());
        }
        let last = d[pos] & 0x80 != 0;
        let btype = d[pos] & 0x7F;
        let len =
            (((d[pos + 1] as u32) << 16) | ((d[pos + 2] as u32) << 8) | d[pos + 3] as u32) as usize;
        let start = pos + 4;
        let block = d.get(start..start + len).ok_or("FLAC truncated block")?;
        match btype {
            0 => {
                if block.len() < 34 {
                    return Err("FLAC STREAMINFO too short".into());
                }
                let sr = (((block[10] as u32) << 12)
                    | ((block[11] as u32) << 4)
                    | ((block[12] as u32) >> 4)) as u64;
                let ch = (((block[12] >> 1) & 0x07) + 1) as u64;
                let total = (((block[13] as u64) & 0x0F) << 32)
                    | ((block[14] as u64) << 24)
                    | ((block[15] as u64) << 16)
                    | ((block[16] as u64) << 8)
                    | (block[17] as u64);
                m.set("sample_rate", FVal::U(sr));
                m.set("channels", FVal::U(ch));
                if sr > 0 && total > 0 {
                    m.set("duration_sec", FVal::F(total as f64 / sr as f64));
                }
            }
            4 => parse_vorbis_comment(block, m),
            _ => {}
        }
        pos = start + len;
        if last {
            break;
        }
    }
    Ok(())
}

// ---------- OGG Vorbis ----------
fn parse_ogg(d: &[u8], m: &mut Meta) -> Result<(), String> {
    if !d.starts_with(b"OggS") {
        return Err("not an Ogg stream (missing OggS)".into());
    }
    let find =
        |needle: &[u8]| -> Option<usize> { d.windows(needle.len()).position(|w| w == needle) };
    if let Some(p) = find(b"\x01vorbis") {
        // After the 7-byte packet identifier: version (4 bytes),
        // channels (1 byte), sample rate (u32 LE).
        let base = p + 7;
        if let Some(ch) = d.get(base + 4) {
            m.set("channels", FVal::U(*ch as u64));
        }
        if let Some(sr) = u32le(d, base + 5) {
            m.set("sample_rate", FVal::U(sr as u64));
        }
    } else {
        return Err("Ogg stream is not Vorbis (no \\x01vorbis header)".into());
    }
    if let Some(p) = find(b"\x03vorbis") {
        parse_vorbis_comment(&d[p + 7..], m);
    }
    Ok(())
}

// ---------- MP4 / MOV ----------
fn parse_mp4(d: &[u8], m: &mut Meta) -> Result<(), String> {
    walk_atoms(d, 0, m, 0);
    if m.fields.is_empty() {
        return Err("MP4: no mvhd/tkhd atoms found".into());
    }
    Ok(())
}

fn atom_header(d: &[u8], pos: usize) -> Option<(String, usize, usize)> {
    // (type, header_len, total_len)
    let size32 = u32be(d, pos)? as usize;
    let typ = latin1(d.get(pos + 4..pos + 8)?);
    let (hdr, total) = if size32 == 1 {
        let sz = u64be(d, pos + 8)? as usize;
        (16usize, sz)
    } else if size32 == 0 {
        (8usize, d.len() - pos)
    } else {
        (8usize, size32)
    };
    if total < hdr || pos + total > d.len() {
        return None;
    }
    Some((typ, hdr, total))
}

fn walk_atoms(d: &[u8], depth: usize, m: &mut Meta, container_end: usize) {
    // This function operates on the slice `d`; `container_end` bounds it
    // (0 means "to the end of the slice", used for the top-level call).
    let mut p = 0usize;
    let limit = if container_end == 0 {
        d.len()
    } else {
        container_end.min(d.len())
    };
    while p + 8 <= limit {
        let (typ, hdr, total) = match atom_header(&d[..limit], p) {
            Some(v) => v,
            None => break,
        };
        let data = &d[p + hdr..p + total];
        match typ.as_str() {
            "mvhd" => parse_mvhd(data, m),
            "tkhd" => {
                if data.len() >= 8 {
                    let w = u32be(data, data.len() - 8).unwrap_or(0) >> 16;
                    let h = u32be(data, data.len() - 4).unwrap_or(0) >> 16;
                    if m.get("width").is_none() {
                        m.set("width", FVal::U(w as u64));
                        m.set("height", FVal::U(h as u64));
                    }
                }
            }
            "moov" | "trak" | "mdia" if depth < 4 => {
                walk_atoms(data, depth + 1, m, data.len());
            }
            _ => {}
        }
        if total == 0 {
            break;
        }
        p += total;
    }
}

fn parse_mvhd(data: &[u8], m: &mut Meta) {
    if data.is_empty() {
        return;
    }
    let version = data[0];
    let (timescale, duration): (u64, u64) = if version == 1 {
        match (u32be(data, 20), u64be(data, 24)) {
            (Some(t), Some(d)) => (t as u64, d),
            _ => return,
        }
    } else {
        match (u32be(data, 12), u32be(data, 16)) {
            (Some(t), Some(d)) => (t as u64, d as u64),
            _ => return,
        }
    };
    if timescale > 0 {
        m.set("timescale", FVal::U(timescale));
        m.set("duration_units", FVal::U(duration));
        m.set("duration_sec", FVal::F(duration as f64 / timescale as f64));
    }
}

// ---------- MKV / WebM (minimal EBML) ----------
fn ebml_vint(d: &[u8], pos: usize) -> Option<(u64, usize, bool)> {
    // (value with marker stripped, length, is_unknown)
    let first = *d.get(pos)?;
    if first == 0 {
        return None;
    }
    let len = first.leading_zeros() as usize + 1;
    if len > 8 || pos + len > d.len() {
        return None;
    }
    let mask: u8 = 0xFF >> len;
    let mut val = (first & mask) as u64;
    let mut all_ones = first & mask == mask;
    for i in 1..len {
        let b = d[pos + i];
        val = (val << 8) | b as u64;
        if b != 0xFF {
            all_ones = false;
        }
    }
    Some((val, len, all_ones))
}
fn ebml_id(d: &[u8], pos: usize) -> Option<(u32, usize)> {
    let first = *d.get(pos)?;
    if first == 0 {
        return None;
    }
    let len = first.leading_zeros() as usize + 1;
    if len > 4 || pos + len > d.len() {
        return None;
    }
    let mut id: u32 = 0;
    for i in 0..len {
        id = (id << 8) | d[pos + i] as u32;
    }
    Some((id, len))
}

fn parse_mkv(d: &[u8], m: &mut Meta) -> Result<(), String> {
    if d.len() < 4 || &d[0..4] != b"\x1a\x45\xdf\xa3" {
        return Err("not a Matroska/WebM file (missing EBML header)".into());
    }
    // Walk elements recursively (depth-limited), slices are regions.
    fn walk(
        d: &[u8],
        depth: usize,
        doc: &mut Option<String>,
        scale: &mut u64,
        dur: &mut Option<f64>,
    ) {
        let mut pos = 0usize;
        while pos < d.len() {
            let (id, idlen) = match ebml_id(d, pos) {
                Some(v) => v,
                None => return,
            };
            let (size, slen, unknown) = match ebml_vint(d, pos + idlen) {
                Some(v) => v,
                None => return,
            };
            let data_start = pos + idlen + slen;
            if data_start > d.len() {
                return;
            }
            let avail = d.len() - data_start;
            let size_usize = if unknown { avail } else { size as usize };
            let data_end = (data_start + size_usize).min(d.len());
            let data = &d[data_start..data_end];
            match id {
                0x4282 => *doc = Some(latin1(data).trim().to_string()),
                0x002A_D7B1 => {
                    let mut v: u64 = 0;
                    for &b in data {
                        v = (v << 8) | b as u64;
                    }
                    *scale = v;
                }
                0x4489 => {
                    if data.len() == 4 {
                        *dur =
                            Some(f32::from_be_bytes([data[0], data[1], data[2], data[3]]) as f64);
                    } else if data.len() == 8 {
                        *dur = Some(f64::from_be_bytes([
                            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                        ]));
                    }
                }
                0x1A45_DFA3 | 0x1853_8067 | 0x1549_A966 if depth < 4 => {
                    walk(data, depth + 1, doc, scale, dur);
                }
                _ => {}
            }
            if unknown {
                // Unknown size consumes the rest of this region; descend
                // was attempted above for Segment, otherwise stop.
                if id != 0x1853_8067 {
                    return;
                }
            }
            if size_usize == 0 && !unknown {
                return; // avoid an infinite loop on corrupt data
            }
            pos = data_end;
        }
    }
    let mut doc = None;
    let mut scale: u64 = 1_000_000;
    let mut dur = None;
    walk(d, 0, &mut doc, &mut scale, &mut dur);
    if let Some(doc) = doc {
        m.set("doc_type", FVal::S(doc));
    } else {
        return Err("MKV: EBML DocType not found".into());
    }
    if let Some(dur) = dur {
        m.set("duration_sec", FVal::F(dur * scale as f64 / 1e9));
    }
    Ok(())
}

// ---------- PDF ----------
fn parse_pdf(d: &[u8], m: &mut Meta) -> Result<(), String> {
    if !d.starts_with(b"%PDF-") {
        return Err("not a PDF (missing %PDF-)".into());
    }
    let header_end = d
        .iter()
        .position(|&b| b == b'\n' || b == b'\r')
        .unwrap_or(d.len().min(16));
    let version = latin1(&d[5..header_end]).trim().to_string();
    m.set("version", FVal::S(version));
    // Documented approximation: count "/Type /Page" / "/Type/Page"
    // occurrences whose "Page" is not the start of "Pages".
    let text = latin1(d);
    let mut pages: u64 = 0;
    for pat in ["/Type /Page", "/Type/Page"] {
        let mut from = 0usize;
        while let Some(idx) = text[from..].find(pat) {
            let at = from + idx;
            let after = at + pat.len();
            if text.as_bytes().get(after) != Some(&b's') {
                pages += 1;
            }
            from = after;
        }
    }
    m.set("pages", FVal::U(pages));
    for (key, field) in [("/Title", "title"), ("/Author", "author")] {
        if let Some(v) = pdf_literal_after(&text, key) {
            m.set(field, FVal::S(v));
        }
    }
    Ok(())
}

fn pdf_literal_after(text: &str, key: &str) -> Option<String> {
    let idx = text.find(key)?;
    let rest = &text[idx + key.len()..];
    let rest = rest.trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let mut out = String::new();
    let mut chars = rest[1..].chars();
    let mut depth = 1usize;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.push(match n {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        other => other,
                    });
                }
            }
            '(' => {
                depth += 1;
                out.push(c);
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(out);
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    None
}

// ---------- Text ----------
fn parse_text(d: &[u8], m: &mut Meta) -> Result<(), String> {
    let (encoding, text): (&str, String) = if d.starts_with(&[0xEF, 0xBB, 0xBF]) {
        ("utf8", String::from_utf8_lossy(&d[3..]).into_owned())
    } else if d.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = d[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        ("utf16le", String::from_utf16_lossy(&units))
    } else if d.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = d[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        ("utf16be", String::from_utf16_lossy(&units))
    } else if std::str::from_utf8(d).is_ok() {
        ("utf8", String::from_utf8_lossy(d).into_owned())
    } else {
        ("non-utf8", String::from_utf8_lossy(d).into_owned())
    };
    m.set("encoding", FVal::S(encoding.to_string()));
    let lines = if text.is_empty() {
        0
    } else {
        text.lines().count() as u64
    };
    let words = text.split_whitespace().count() as u64;
    m.set("lines", FVal::U(lines));
    m.set("words", FVal::U(words));
    Ok(())
}

// ---------- Writing ----------

/// Persist `new_bytes` over `path`, keeping a `<path>.bak` copy of the
/// original first. Temp file in the same directory + rename.
fn atomic_replace(path: &str, new_bytes: &[u8]) -> MResult<String> {
    let p = Path::new(path);
    let original = std::fs::read(p).map_err(|e| (ERR_PLATFORM, format!("read: {e}")))?;
    let bak = format!("{path}.bak");
    std::fs::write(&bak, &original).map_err(|e| (ERR_PLATFORM, format!("backup: {e}")))?;
    let tmp = format!("{path}.tmp-fm");
    std::fs::write(&tmp, new_bytes).map_err(|e| (ERR_PLATFORM, format!("temp write: {e}")))?;
    std::fs::rename(&tmp, p).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        (ERR_PLATFORM, format!("replace: {e}"))
    })?;
    Ok(bak)
}

fn applied_json(format: &str, applied: &[(&str, String)], bak: &str) -> String {
    let mut inner = String::new();
    for (i, (k, v)) in applied.iter().enumerate() {
        if i > 0 {
            inner.push(',');
        }
        inner.push_str(&format!("\"{}\":\"{}\"", json_escape(k), json_escape(v)));
    }
    format!(
        "{{\"format\":\"{}\",\"applied\":{{{}}},\"backup\":\"{}\"}}",
        json_escape(format),
        inner,
        json_escape(bak)
    )
}

pub fn write_file_json(path: &str, edits_json: &str) -> MResult<String> {
    if path.is_empty() || edits_json.is_empty() {
        return Err((ERR_INVALID_ARG, "path and edits_json are required".into()));
    }
    let parsed = crate::json::parse(edits_json)
        .map_err(|e| (ERR_INVALID_ARG, format!("edits JSON: {e}")))?;
    let obj = match &parsed {
        crate::json::Value::Object(pairs) => pairs.clone(),
        _ => return Err((ERR_INVALID_ARG, "edits must be a JSON object".into())),
    };
    if obj.is_empty() {
        return Err((ERR_INVALID_ARG, "edits object is empty".into()));
    }
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "mp3" => write_id3v1(path, &obj),
        "png" => write_png_text(path, &obj),
        _ => Err((
            ERR_NOT_SUPPORTED,
            format!(
                "editing is not supported for \".{ext}\" files (supported: MP3 ID3v1 tags, PNG tEXt chunks)"
            ),
        )),
    }
}

fn write_id3v1(path: &str, edits: &[(String, crate::json::Value)]) -> MResult<String> {
    const ALLOWED: &[&str] = &[
        "title", "artist", "album", "year", "comment", "track", "genre",
    ];
    for (k, _) in edits {
        if !ALLOWED.contains(&k.as_str()) {
            return Err((ERR_NOT_SUPPORTED, format!("ID3v1 has no \"{k}\" field")));
        }
    }
    let data = std::fs::read(path).map_err(|e| {
        let code = if e.kind() == std::io::ErrorKind::NotFound {
            ERR_NOT_FOUND
        } else {
            ERR_PLATFORM
        };
        (code, format!("cannot read \"{path}\": {e}"))
    })?;
    let has_tag = data.len() >= 128 && &data[data.len() - 128..data.len() - 125] == b"TAG";
    // Start from the existing tag so unedited fields survive.
    let mut tag = if has_tag {
        data[data.len() - 128..].to_vec()
    } else {
        let mut t = vec![0u8; 128];
        t[0..3].copy_from_slice(b"TAG");
        t
    };
    let put = |tag: &mut [u8], a: usize, b: usize, val: &str| {
        for slot in &mut tag[a..b] {
            *slot = 0;
        }
        for (i, ch) in val.chars().take(b - a).enumerate() {
            tag[a + i] = if (ch as u32) <= 0xFF { ch as u8 } else { b'?' };
        }
    };
    let mut applied: Vec<(&str, String)> = Vec::new();
    // Need owned keys for the applied list lifetime — use static keys via match.
    for (k, v) in edits {
        let val = v
            .scalar_string()
            .ok_or_else(|| (ERR_INVALID_ARG, format!("edit \"{k}\" must be a scalar")))?;
        match k.as_str() {
            "title" => {
                put(&mut tag, 3, 33, &val);
                applied.push(("title", val));
            }
            "artist" => {
                put(&mut tag, 33, 63, &val);
                applied.push(("artist", val));
            }
            "album" => {
                put(&mut tag, 63, 93, &val);
                applied.push(("album", val));
            }
            "year" => {
                put(&mut tag, 93, 97, &val);
                applied.push(("year", val));
            }
            "comment" => {
                // Preserve the track-marker layout if a track is present.
                if tag[125] == 0 && tag[126] != 0 {
                    put(&mut tag, 97, 125, &val);
                } else {
                    put(&mut tag, 97, 127, &val);
                }
                applied.push(("comment", val));
            }
            "track" => {
                let n: u32 = val
                    .trim()
                    .parse()
                    .map_err(|_| (ERR_INVALID_ARG, "track must be a number 1-255".into()))?;
                if n == 0 || n > 255 {
                    return Err((ERR_INVALID_ARG, "track must be 1-255".into()));
                }
                // Shrink comment to 28 bytes to make room for the marker.
                for slot in &mut tag[125..127] {
                    *slot = 0;
                }
                tag[126] = n as u8;
                applied.push(("track", val));
            }
            "genre" => {
                let byte = if let Ok(n) = val.trim().parse::<u32>() {
                    if n > 255 {
                        return Err((ERR_INVALID_ARG, "genre number must be 0-255".into()));
                    }
                    n as u8
                } else if let Some(idx) = GENRES.iter().position(|g| g.eq_ignore_ascii_case(&val)) {
                    idx as u8
                } else {
                    return Err((
                        ERR_INVALID_ARG,
                        format!("unknown genre \"{val}\" (use a genre name or a number 0-255)"),
                    ));
                };
                tag[127] = byte;
                applied.push(("genre", val));
            }
            _ => unreachable!(),
        }
    }
    let mut new_data = if has_tag {
        data[..data.len() - 128].to_vec()
    } else {
        data.clone()
    };
    new_data.extend_from_slice(&tag);
    let bak = atomic_replace(path, &new_data)?;
    Ok(applied_json("id3v1", &applied, &bak))
}

fn write_png_text(path: &str, edits: &[(String, crate::json::Value)]) -> MResult<String> {
    for (k, v) in edits {
        if k.is_empty() || k.len() > 79 || k.contains('\0') {
            return Err((
                ERR_INVALID_ARG,
                "PNG tEXt keywords must be 1-79 bytes with no NUL".into(),
            ));
        }
        let val = v
            .as_str()
            .ok_or_else(|| (ERR_INVALID_ARG, "PNG tEXt values must be strings".into()))?;
        if val.chars().any(|c| (c as u32) > 0xFF) {
            return Err((
                ERR_INVALID_ARG,
                "PNG tEXt values must be Latin-1 text".into(),
            ));
        }
        if k.chars().any(|c| (c as u32) > 0xFF || c == '\0') {
            return Err((
                ERR_INVALID_ARG,
                "PNG tEXt keywords must be Latin-1 text".into(),
            ));
        }
    }
    let data = std::fs::read(path).map_err(|e| {
        let code = if e.kind() == std::io::ErrorKind::NotFound {
            ERR_NOT_FOUND
        } else {
            ERR_PLATFORM
        };
        (code, format!("cannot read \"{path}\": {e}"))
    })?;
    if data.len() < 8 || &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err((ERR_INVALID_ARG, "not a PNG file".into()));
    }
    // Split into raw chunks (length+type+data+crc slices), validating bounds.
    let mut chunks: Vec<&[u8]> = Vec::new();
    let mut pos = 8usize;
    let mut saw_iend = false;
    while pos + 8 <= data.len() {
        let len = match u32be(&data, pos) {
            Some(v) => v as usize,
            None => return Err((ERR_INVALID_ARG, "PNG truncated chunk".into())),
        };
        let total = 8usize
            .checked_add(len)
            .and_then(|t| t.checked_add(4))
            .filter(|&t| pos + t <= data.len())
            .ok_or((ERR_INVALID_ARG, "PNG truncated chunk".to_string()))?;
        chunks.push(&data[pos..pos + total]);
        if &data[pos + 4..pos + 8] == b"IEND" {
            saw_iend = true;
            break;
        }
        pos += total;
    }
    if !saw_iend {
        return Err((ERR_INVALID_ARG, "PNG has no IEND chunk".into()));
    }
    let keys: Vec<&str> = edits.iter().map(|(k, _)| k.as_str()).collect();
    let mut out: Vec<u8> = data[0..8].to_vec();
    for chunk in &chunks {
        let ctype = &chunk[4..8];
        if ctype == b"IEND" {
            // New tEXt chunks go immediately before IEND.
            for (k, v) in edits {
                let val = v.as_str().unwrap_or("");
                let mut cdata = Vec::with_capacity(k.len() + 1 + val.len());
                for ch in k.chars() {
                    cdata.push(ch as u8);
                }
                cdata.push(0);
                for ch in val.chars() {
                    cdata.push(ch as u8);
                }
                let mut crc_input = b"tEXt".to_vec();
                crc_input.extend_from_slice(&cdata);
                let crc = crc32(&crc_input);
                out.extend_from_slice(&(cdata.len() as u32).to_be_bytes());
                out.extend_from_slice(b"tEXt");
                out.extend_from_slice(&cdata);
                out.extend_from_slice(&crc.to_be_bytes());
            }
            out.extend_from_slice(chunk);
        } else if ctype == b"tEXt" {
            // Drop a tEXt chunk whose keyword is being replaced.
            let cdata = &chunk[8..chunk.len() - 4];
            let kw_end = cdata.iter().position(|&b| b == 0).unwrap_or(cdata.len());
            let kw = latin1(&cdata[..kw_end]);
            if keys.contains(&kw.as_str()) {
                continue;
            }
            out.extend_from_slice(chunk);
        } else {
            out.extend_from_slice(chunk);
        }
    }
    let applied: Vec<(&str, String)> = edits
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str().unwrap_or("").to_string()))
        .collect();
    let bak = atomic_replace(path, &out)?;
    Ok(applied_json("png_text", &applied, &bak))
}
