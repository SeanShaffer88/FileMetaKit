//! Integration tests: fixtures are built byte-by-byte (no binary blobs
//! on disk), written to a temp directory, and read back through the
//! same public functions the FFI exports call.

use file_meta_kit::meta::{
    compare_file_json, crc32, read_file_json, read_metadata_bytes, write_file_json,
};
use std::path::PathBuf;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fmk-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn save(name: &str, fname: &str, bytes: &[u8]) -> String {
    let p = dir(name).join(fname);
    std::fs::write(&p, bytes).unwrap();
    p.to_string_lossy().into_owned()
}

// ---------- fixture builders ----------

fn png_chunk(typ: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&(data.len() as u32).to_be_bytes());
    v.extend_from_slice(typ);
    v.extend_from_slice(data);
    let mut crc_in = typ.to_vec();
    crc_in.extend_from_slice(data);
    v.extend_from_slice(&crc32(&crc_in).to_be_bytes());
    v
}
fn png_fixture(text: Option<(&str, &str)>, w: u32, h: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    v.extend_from_slice(&png_chunk(b"IHDR", &ihdr));
    if let Some((k, val)) = text {
        let mut td = k.as_bytes().to_vec();
        td.push(0);
        td.extend_from_slice(val.as_bytes());
        v.extend_from_slice(&png_chunk(b"tEXt", &td));
    }
    v.extend_from_slice(&png_chunk(b"IEND", &[]));
    v
}

fn jpeg_fixture() -> Vec<u8> {
    // TIFF (little endian), IFD0 at offset 8, 4 entries.
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II");
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&8u32.to_le_bytes());
    let data_off = 8 + 2 + 4 * 12 + 4; // 62
    let make = b"Canon\0";
    let dt = b"2024:01:02 03:04:05\0";
    let dt_off = data_off + make.len();
    tiff.extend_from_slice(&4u16.to_le_bytes());
    let entry = |tag: u16, typ: u16, count: u32, val: [u8; 4]| {
        let mut e = Vec::new();
        e.extend_from_slice(&tag.to_le_bytes());
        e.extend_from_slice(&typ.to_le_bytes());
        e.extend_from_slice(&count.to_le_bytes());
        e.extend_from_slice(&val);
        e
    };
    let off_bytes = |v: u32| v.to_le_bytes();
    tiff.extend_from_slice(&entry(0x010F, 2, 6, off_bytes(data_off as u32)));
    tiff.extend_from_slice(&entry(0x0110, 2, 4, *b"Cam\0"));
    tiff.extend_from_slice(&entry(0x0112, 3, 1, [1, 0, 0, 0]));
    tiff.extend_from_slice(&entry(0x0132, 2, 20, off_bytes(dt_off as u32)));
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(make);
    tiff.extend_from_slice(dt);
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(&tiff);

    let mut v = vec![0xFF, 0xD8];
    v.push(0xFF);
    v.push(0xE1);
    v.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
    v.extend_from_slice(&app1);
    // SOF0: 200 wide, 100 high
    let sof: [u8; 9] = [8, 0, 100, 0, 200, 1, 1, 0x11, 0];
    v.push(0xFF);
    v.push(0xC0);
    v.extend_from_slice(&((sof.len() + 2) as u16).to_be_bytes());
    v.extend_from_slice(&sof);
    v.extend_from_slice(&[0xFF, 0xD9]);
    v
}

fn gif_fixture(frames: usize) -> Vec<u8> {
    let mut v = b"GIF89a".to_vec();
    v.extend_from_slice(&3u16.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&[0, 0, 0]); // packed, bg, aspect
    for _ in 0..frames {
        v.push(0x2C);
        v.extend_from_slice(&[0, 0, 0, 0, 3, 0, 2, 0, 0]); // descriptor
        v.push(2); // LZW min code size
        v.extend_from_slice(&[1, 0x00, 0]); // one sub-block + terminator
    }
    v.push(0x3B);
    v
}

fn wav_fixture() -> Vec<u8> {
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes()); // PCM
    fmt.extend_from_slice(&1u16.to_le_bytes()); // channels
    fmt.extend_from_slice(&8000u32.to_le_bytes()); // sample rate
    fmt.extend_from_slice(&8000u32.to_le_bytes()); // byte rate
    fmt.extend_from_slice(&1u16.to_le_bytes());
    fmt.extend_from_slice(&8u16.to_le_bytes()); // bits
    let data = vec![0u8; 8000]; // exactly 1 second
    let mut riff_body = b"WAVE".to_vec();
    riff_body.extend_from_slice(b"fmt ");
    riff_body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    riff_body.extend_from_slice(&fmt);
    riff_body.extend_from_slice(b"data");
    riff_body.extend_from_slice(&(data.len() as u32).to_le_bytes());
    riff_body.extend_from_slice(&data);
    let mut v = b"RIFF".to_vec();
    v.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
    v.extend_from_slice(&riff_body);
    v
}

fn id3v2_frame(id: &str, text: &str) -> Vec<u8> {
    let mut payload = vec![3u8]; // UTF-8 encoding byte
    payload.extend_from_slice(text.as_bytes());
    let mut f = id.as_bytes().to_vec();
    f.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    f.extend_from_slice(&[0, 0]);
    f.extend_from_slice(&payload);
    f
}
fn mp3_fixture(with_v1: bool) -> Vec<u8> {
    let mut frames = Vec::new();
    frames.extend_from_slice(&id3v2_frame("TIT2", "V2 Title"));
    frames.extend_from_slice(&id3v2_frame("TPE1", "V2 Artist"));
    frames.extend_from_slice(&id3v2_frame("TCON", "Rock"));
    let syncsafe = |n: usize| {
        [
            ((n >> 21) & 0x7F) as u8,
            ((n >> 14) & 0x7F) as u8,
            ((n >> 7) & 0x7F) as u8,
            (n & 0x7F) as u8,
        ]
    };
    let mut v = b"ID3".to_vec();
    v.extend_from_slice(&[3, 0, 0]);
    v.extend_from_slice(&syncsafe(frames.len()));
    v.extend_from_slice(&frames);
    // MPEG1 Layer III, 128 kbps, 44100 Hz + payload
    v.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
    v.extend_from_slice(&vec![0u8; 16000]);
    if with_v1 {
        let mut tag = vec![0u8; 128];
        tag[0..3].copy_from_slice(b"TAG");
        let put = |tag: &mut [u8], a: usize, s: &str| {
            tag[a..a + s.len()].copy_from_slice(s.as_bytes());
        };
        put(&mut tag, 3, "V1 Song");
        put(&mut tag, 33, "V1 Artist");
        put(&mut tag, 93, "1999");
        tag[127] = 17; // Rock
        v.extend_from_slice(&tag);
    }
    v
}

fn flac_fixture() -> Vec<u8> {
    let mut si = vec![0u8; 34];
    let sr: u32 = 44100;
    let total: u64 = 44100; // 1 second
    si[10] = (sr >> 12) as u8;
    si[11] = (sr >> 4) as u8;
    si[12] = (((sr & 0xF) << 4) | (1 << 1)) as u8; // 2 channels (value 1)
    si[13] = ((total >> 32) & 0xF) as u8;
    si[14] = (total >> 24) as u8;
    si[15] = (total >> 16) as u8;
    si[16] = (total >> 8) as u8;
    si[17] = total as u8;
    let entry = b"TITLE=Flac Song";
    let mut comment = Vec::new();
    comment.extend_from_slice(&0u32.to_le_bytes()); // vendor len
    comment.extend_from_slice(&1u32.to_le_bytes()); // count
    comment.extend_from_slice(&(entry.len() as u32).to_le_bytes());
    comment.extend_from_slice(entry);
    let mut v = b"fLaC".to_vec();
    v.push(0); // STREAMINFO, not last
    v.extend_from_slice(&[0, 0, 34]);
    v.extend_from_slice(&si);
    v.push(0x84); // VORBIS_COMMENT, last
    let len = comment.len();
    v.extend_from_slice(&[(len >> 16) as u8, (len >> 8) as u8, len as u8]);
    v.extend_from_slice(&comment);
    v
}

fn atom(typ: &str, data: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(&((data.len() + 8) as u32).to_be_bytes());
    v.extend_from_slice(typ.as_bytes());
    v.extend_from_slice(data);
    v
}
fn mp4_fixture() -> Vec<u8> {
    let mut mvhd = vec![0u8; 100];
    // v0: timescale at 12, duration at 16
    mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&2500u32.to_be_bytes());
    let mut tkhd = vec![0u8; 92];
    let n = tkhd.len();
    tkhd[n - 8..n - 4].copy_from_slice(&(640u32 << 16).to_be_bytes());
    tkhd[n - 4..n].copy_from_slice(&(360u32 << 16).to_be_bytes());
    let trak = atom("trak", &atom("tkhd", &tkhd));
    let mut moov_data = atom("mvhd", &mvhd);
    moov_data.extend_from_slice(&trak);
    atom("moov", &moov_data)
}

fn pdf_fixture() -> Vec<u8> {
    b"%PDF-1.4\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
      2 0 obj << /Type /Pages /Count 2 >> endobj\n\
      3 0 obj << /Type /Page /Parent 2 0 R >> endobj\n\
      4 0 obj << /Type /Page /Parent 2 0 R >> endobj\n\
      5 0 obj << /Title (My Doc) /Author (Jane) >> endobj\n"
        .to_vec()
}

fn webp_vp8l_fixture() -> Vec<u8> {
    let bits: u32 = 99 | (49 << 14); // 100 x 50
    let mut chunk_data = vec![0x2F];
    chunk_data.extend_from_slice(&bits.to_le_bytes());
    let mut riff_body = b"WEBP".to_vec();
    riff_body.extend_from_slice(b"VP8L");
    riff_body.extend_from_slice(&(chunk_data.len() as u32).to_le_bytes());
    riff_body.extend_from_slice(&chunk_data);
    if riff_body.len() % 2 == 1 {
        riff_body.push(0);
    }
    let mut v = b"RIFF".to_vec();
    v.extend_from_slice(&(riff_body.len() as u32).to_le_bytes());
    v.extend_from_slice(&riff_body);
    v
}

// ---------- tests ----------

#[test]
fn png_reads_dims_and_text() {
    let p = save("png", "a.png", &png_fixture(Some(("Title", "Hello")), 4, 5));
    let j = read_file_json(&p).unwrap();
    assert!(j.contains("\"kind\":\"image\""), "{j}");
    assert!(j.contains("\"width\":4"), "{j}");
    assert!(j.contains("\"height\":5"), "{j}");
    assert!(j.contains("\"bit_depth\":8"), "{j}");
    assert!(j.contains("\"Title\":\"Hello\""), "{j}");
}

#[test]
fn jpeg_reads_sof_and_exif_both_endians() {
    let p = save("jpg", "a.jpg", &jpeg_fixture());
    let j = read_file_json(&p).unwrap();
    assert!(j.contains("\"width\":200"), "{j}");
    assert!(j.contains("\"height\":100"), "{j}");
    assert!(j.contains("\"exif_make\":\"Canon\""), "{j}");
    assert!(j.contains("\"exif_model\":\"Cam\""), "{j}");
    assert!(
        j.contains("\"exif_datetime\":\"2024:01:02 03:04:05\""),
        "{j}"
    );
    assert!(j.contains("\"exif_orientation\":1"), "{j}");
    // Big-endian TIFF variant: build by hand would be long; at minimum the
    // LE path is covered and corrupt BE data must not panic (below).
}

#[test]
fn gif_counts_frames() {
    let p = save("gif", "a.gif", &gif_fixture(2));
    let j = read_file_json(&p).unwrap();
    assert!(j.contains("\"frames\":2"), "{j}");
    assert!(j.contains("\"animated\":true"), "{j}");
    assert!(j.contains("\"width\":3"), "{j}");
}

#[test]
fn wav_reads_duration() {
    let p = save("wav", "a.wav", &wav_fixture());
    let j = read_file_json(&p).unwrap();
    assert!(j.contains("\"sample_rate\":8000"), "{j}");
    assert!(j.contains("\"channels\":1"), "{j}");
    assert!(j.contains("\"duration_sec\":1"), "{j}");
}

#[test]
fn mp3_reads_id3v2_frame_and_v1_fallback() {
    let p = save("mp3", "a.mp3", &mp3_fixture(true));
    let j = read_file_json(&p).unwrap();
    assert!(j.contains("\"title\":\"V2 Title\""), "{j}"); // v2 wins over v1
    assert!(j.contains("\"artist\":\"V2 Artist\""), "{j}");
    assert!(j.contains("\"year\":\"1999\""), "{j}"); // v1 fallback fills gaps
    assert!(j.contains("\"bitrate_kbps\":128"), "{j}");
    assert!(j.contains("\"sample_rate\":44100"), "{j}");
    assert!(j.contains("\"duration_sec\":"), "{j}");
}

#[test]
fn flac_reads_streaminfo_and_comment() {
    let p = save("flac", "a.flac", &flac_fixture());
    let j = read_file_json(&p).unwrap();
    assert!(j.contains("\"sample_rate\":44100"), "{j}");
    assert!(j.contains("\"channels\":2"), "{j}");
    assert!(j.contains("\"duration_sec\":1"), "{j}");
    assert!(j.contains("\"title\":\"Flac Song\""), "{j}");
}

#[test]
fn mp4_reads_mvhd_and_tkhd() {
    let p = save("mp4", "a.mp4", &mp4_fixture());
    let j = read_file_json(&p).unwrap();
    assert!(j.contains("\"duration_sec\":2.5"), "{j}");
    assert!(j.contains("\"timescale\":1000"), "{j}");
    assert!(j.contains("\"width\":640"), "{j}");
    assert!(j.contains("\"height\":360"), "{j}");
}

#[test]
fn pdf_reads_version_pages_and_info() {
    let p = save("pdf", "a.pdf", &pdf_fixture());
    let j = read_file_json(&p).unwrap();
    assert!(j.contains("\"version\":\"1.4\""), "{j}");
    assert!(j.contains("\"pages\":2"), "{j}");
    assert!(j.contains("\"title\":\"My Doc\""), "{j}");
    assert!(j.contains("\"author\":\"Jane\""), "{j}");
}

#[test]
fn webp_vp8l_and_text_and_bmp() {
    let j = read_file_json(&save("webp", "a.webp", &webp_vp8l_fixture())).unwrap();
    assert!(j.contains("\"width\":100"), "{j}");
    assert!(j.contains("\"height\":50"), "{j}");
    let txt = "one two\nthree\n".as_bytes();
    let m = read_metadata_bytes("notes.md", txt, 0);
    let j = m.to_json();
    assert!(j.contains("\"lines\":2"), "{j}");
    assert!(j.contains("\"words\":3"), "{j}");
    assert!(j.contains("\"encoding\":\"utf8\""), "{j}");
    // BMP: BM + DIB 40
    let mut bmp = b"BM".to_vec();
    bmp.extend_from_slice(&[0u8; 12]);
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&7i32.to_le_bytes());
    bmp.extend_from_slice(&9i32.to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&24u16.to_le_bytes());
    let j = read_metadata_bytes("x.bmp", &bmp, 0).to_json();
    assert!(j.contains("\"width\":7"), "{j}");
    assert!(j.contains("\"bpp\":24"), "{j}");
}

#[test]
fn corrupt_and_truncated_never_panic() {
    for (name, bytes) in [
        ("bad.png", &png_fixture(None, 1, 1)[..10]),
        ("bad.jpg", &[0xFF, 0xD8, 0xFF, 0xE0, 0x00][..]),
        ("bad.mp3", &[0u8; 16][..]),
        ("bad.flac", &b"fLaC\x00"[..]),
        ("bad.pdf", &b"%PDF-"[..]),
        ("bad.mp4", &[0, 0, 0, 8][..]),
    ] {
        let j = read_file_json(&save("corrupt", name, bytes)).unwrap();
        assert!(
            j.contains("\"parse_error\"") || j.contains("\"supported\":"),
            "{name}: {j}"
        );
    }
    // A supported extension with wrong magic reports parse_error.
    let j = read_file_json(&save("corrupt", "fake.png", b"not a png at all....")).unwrap();
    assert!(j.contains("\"parse_error\""), "{j}");
}

#[test]
fn unsupported_is_graceful() {
    let j = read_file_json(&save("unsup", "data.xyz", b"hello")).unwrap();
    assert!(j.contains("\"supported\":false"), "{j}");
    assert!(j.contains("\"kind\":\"other\""), "{j}");
    assert!(j.contains("unsupported extension"), "{j}");
}

#[test]
fn id3v1_write_round_trip_and_backup() {
    let orig = mp3_fixture(false);
    let p = save("wmp3", "song.mp3", &orig);
    let out = write_file_json(&p, r#"{"title":"New Title","track":3,"genre":"Rock"}"#).unwrap();
    assert!(out.contains("\"format\":\"id3v1\""), "{out}");
    assert!(std::path::Path::new(&format!("{p}.bak")).exists());
    assert_eq!(
        std::fs::read(format!("{p}.bak")).unwrap(),
        orig,
        "backup holds the original bytes"
    );
    let j = read_file_json(&p).unwrap();
    assert!(j.contains("\"title\":\"V2 Title\""), "v2 still wins: {j}");
    // Remove the v2 precedence question: read the raw tag tail.
    let bytes = std::fs::read(&p).unwrap();
    let tag = &bytes[bytes.len() - 128..];
    assert_eq!(&tag[0..3], b"TAG");
    assert!(std::str::from_utf8(&tag[3..12])
        .unwrap()
        .starts_with("New Title"));
    assert_eq!(tag[126], 3);
    assert_eq!(tag[127], 17);
}

#[test]
fn unsupported_write_is_refused() {
    let p = save("wref", "a.wav", &wav_fixture());
    let err = write_file_json(&p, r#"{"title":"x"}"#).unwrap_err();
    assert_eq!(err.0, file_meta_kit::ERR_NOT_SUPPORTED);
    let p2 = save("wref", "b.mp3", &mp3_fixture(false));
    let err = write_file_json(&p2, r#"{"bogus":"x"}"#).unwrap_err();
    assert_eq!(err.0, file_meta_kit::ERR_NOT_SUPPORTED);
}

#[test]
fn png_text_write_round_trip_with_valid_crc() {
    let orig = png_fixture(Some(("Title", "Old")), 2, 2);
    let p = save("wpng", "img.png", &orig);
    let out = write_file_json(&p, r#"{"Title":"New","Author":"Me"}"#).unwrap();
    assert!(out.contains("\"format\":\"png_text\""), "{out}");
    assert_eq!(std::fs::read(format!("{p}.bak")).unwrap(), orig);
    let j = read_file_json(&p).unwrap();
    assert!(j.contains("\"Title\":\"New\""), "{j}");
    assert!(j.contains("\"Author\":\"Me\""), "{j}");
    assert!(!j.contains("Old"), "old tEXt replaced: {j}");
    // Every chunk CRC in the written file validates.
    let bytes = std::fs::read(&p).unwrap();
    let mut pos = 8usize;
    let mut saw_text = 0;
    while pos + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
            as usize;
        let typ = &bytes[pos + 4..pos + 8];
        let data = &bytes[pos + 8..pos + 8 + len];
        let stored = u32::from_be_bytes([
            bytes[pos + 8 + len],
            bytes[pos + 9 + len],
            bytes[pos + 10 + len],
            bytes[pos + 11 + len],
        ]);
        let mut crc_in = typ.to_vec();
        crc_in.extend_from_slice(data);
        assert_eq!(crc32(&crc_in), stored, "CRC of chunk {:?}", typ);
        if typ == b"tEXt" {
            saw_text += 1;
        }
        pos += 12 + len;
        if typ == b"IEND" {
            break;
        }
    }
    assert_eq!(saw_text, 2);
}

#[test]
fn compare_detects_differences() {
    let a = save("cmp", "a.png", &png_fixture(Some(("Title", "One")), 4, 5));
    let b = save("cmp", "b.png", &png_fixture(Some(("Title", "Two")), 8, 5));
    let j = compare_file_json(&a, &b).unwrap();
    assert!(j.contains("\"same_format\":true"), "{j}");
    assert!(j.contains("\"identical_metadata\":false"), "{j}");
    assert!(j.contains("\"field\":\"width\""), "{j}");
    assert!(j.contains("\"field\":\"Title\""), "{j}");
    let same = compare_file_json(&a, &a).unwrap();
    assert!(same.contains("\"identical_metadata\":true"), "{same}");
    let other = save("cmp", "c.jpg", &jpeg_fixture());
    let j = compare_file_json(&a, &other).unwrap();
    assert!(j.contains("\"same_format\":false"), "{j}");
}

#[test]
fn jpeg_exif_big_endian() {
    // TIFF (big endian), one entry: Make = "Nikon\0" at offset 26.
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"MM");
    tiff.extend_from_slice(&42u16.to_be_bytes());
    tiff.extend_from_slice(&8u32.to_be_bytes());
    tiff.extend_from_slice(&1u16.to_be_bytes());
    tiff.extend_from_slice(&0x010Fu16.to_be_bytes());
    tiff.extend_from_slice(&2u16.to_be_bytes());
    tiff.extend_from_slice(&6u32.to_be_bytes());
    tiff.extend_from_slice(&26u32.to_be_bytes());
    tiff.extend_from_slice(&0u32.to_be_bytes());
    tiff.extend_from_slice(b"Nikon\0");
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(&tiff);
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xE1];
    v.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
    v.extend_from_slice(&app1);
    let sof: [u8; 9] = [8, 0, 10, 0, 20, 1, 1, 0x11, 0];
    v.extend_from_slice(&[0xFF, 0xC0]);
    v.extend_from_slice(&((sof.len() + 2) as u16).to_be_bytes());
    v.extend_from_slice(&sof);
    v.extend_from_slice(&[0xFF, 0xD9]);
    let j = read_file_json(&save("jpgbe", "be.jpg", &v)).unwrap();
    assert!(j.contains("\"exif_make\":\"Nikon\""), "{j}");
    assert!(j.contains("\"width\":20"), "{j}");
}

#[test]
fn ogg_vorbis_reads_header_and_comment() {
    let mut v = b"OggS".to_vec();
    v.extend_from_slice(&[0u8; 23]);
    v.extend_from_slice(b"\x01vorbis");
    v.extend_from_slice(&0u32.to_le_bytes()); // version
    v.push(2); // channels
    v.extend_from_slice(&44100u32.to_le_bytes());
    v.extend_from_slice(&[0u8; 8]);
    v.extend_from_slice(b"\x03vorbis");
    let entry = b"ARTIST=Ogg Artist";
    v.extend_from_slice(&0u32.to_le_bytes()); // vendor len
    v.extend_from_slice(&1u32.to_le_bytes()); // count
    v.extend_from_slice(&(entry.len() as u32).to_le_bytes());
    v.extend_from_slice(entry);
    let j = read_file_json(&save("ogg", "a.ogg", &v)).unwrap();
    assert!(j.contains("\"channels\":2"), "{j}");
    assert!(j.contains("\"sample_rate\":44100"), "{j}");
    assert!(j.contains("\"artist\":\"Ogg Artist\""), "{j}");
}

#[test]
fn mkv_reads_doc_type() {
    // EBML header containing just a DocType element ("matroska").
    let mut ebml_data = vec![0x42, 0x82, 0x88];
    ebml_data.extend_from_slice(b"matroska");
    let mut v = vec![0x1A, 0x45, 0xDF, 0xA3, 0x80 | ebml_data.len() as u8];
    v.extend_from_slice(&ebml_data);
    let j = read_file_json(&save("mkv", "a.mkv", &v)).unwrap();
    assert!(j.contains("\"doc_type\":\"matroska\""), "{j}");
}

#[test]
fn missing_file_is_not_found() {
    let err = read_file_json("/definitely/not/here.mp3").unwrap_err();
    assert_eq!(err.0, file_meta_kit::ERR_NOT_FOUND);
}
