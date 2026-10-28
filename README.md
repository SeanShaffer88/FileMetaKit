# FileMetaKit

## TL;DR

One small DLL that answers "what do we know about this file?" for common images, audio, video, and documents: call `fm_read` with a path, get one JSON object back with the file's size, type, and format-specific facts (image dimensions, song tags, track duration, PDF page count). It can also write the two tag formats that are safe to edit in place — MP3 ID3v1 tags and PNG text chunks — and compare two files' metadata field by field. No dependencies, no crashes on broken files: a corrupt file returns JSON saying what went wrong.

```c
#include "filemetakit.h"
#include <stdio.h>

int main(void) {
    char *json = fm_read("song.mp3");
    if (!json) { printf("error: %s\n", fm_last_error()); return 1; }
    printf("%s\n", json);   /* {"file_name":"song.mp3",...,"fields":{"title":...}} */
    fm_free_string(json);

    char *applied = fm_write("song.mp3", "{\"title\":\"New Title\",\"track\":3}");
    if (!applied) { printf("write error: %s\n", fm_last_error()); return 1; }
    printf("%s\n", applied);
    fm_free_string(applied);
    return 0;
}
```

## Features

- **One read call for many formats.** Every result has the same common block — `file_name`, `extension`, `size`, `modified_unix`, `kind` (`image`/`audio`/`video`/`document`/`other`), `mime`, `supported` — plus a `fields` object with format-specific facts.
- **Images:** PNG dimensions, bit depth, colour type, and `tEXt` entries; JPEG dimensions from the frame header and EXIF camera make, model, date/time, and orientation (both TIFF byte orders); GIF dimensions, frame count, and an `animated` flag; BMP dimensions and bits per pixel; WebP dimensions from VP8X, VP8L, or VP8 chunks.
- **Audio:** WAV duration, sample rate, channels, and bit depth; MP3 ID3v2 tags (title, artist, album, year, track, genre) with ID3v1 fallback, plus bitrate, sample rate, and a duration estimate from the first MPEG frame header and file size; FLAC sample rate, channels, and duration from STREAMINFO plus Vorbis comment tags; Ogg Vorbis sample rate, channels, and comment tags.
- **Video:** MP4/MOV duration and timescale from `mvhd` and picture size from `tkhd`; MKV/WebM EBML document type (`matroska`/`webm`) and duration when the Info element carries one.
- **Documents:** PDF version, a page count (see Limitations), and title/author when the Info dictionary holds them as plain literal strings; plain text and Markdown line and word counts plus an encoding guess (`utf8`, `utf16le`, `utf16be`, or `non-utf8`).
- **Safe writes, two formats only.** MP3: write or replace the ID3v1 tag. PNG: insert or replace `tEXt` chunks with correct CRCs. Every write keeps a `<path>.bak` copy of the original, writes a temporary file in the same folder, then renames it over the original. Anything else returns `FM_ERR_NOT_SUPPORTED` with the reason in `fm_last_error()`.
- **Compare.** `fm_compare(a, b)` returns whether the extensions match, a list of differing fields with both values, and whether the metadata is identical.
- **Broken files never crash.** Every parser is bounds-checked. A truncated or corrupt file in a supported format returns JSON with a `parse_error` field explaining the problem; an unsupported extension returns `"supported":false` with a `reason`, and the common block is still filled in.

## Build

Requirements: Rust 1.80 or newer (zero crates — nothing is downloaded).

```
cargo build --release
cargo test
```

That produces the shared library (`filemetakit.dll` on Windows, `libfile_meta_kit.so` / `.dylib` elsewhere), a static library, and the Rust library. The C header is `include/filemetakit.h`; it documents every export, the JSON shapes, and the ownership rule (free returned strings with `fm_free_string`).

## The API

| Call | Purpose |
|---|---|
| `fm_version()` | Version string (static — do not free). |
| `fm_read(path)` | Metadata JSON for one file. |
| `fm_write(path, edits_json)` | Apply supported edits; returns JSON of what was applied. |
| `fm_compare(path_a, path_b)` | JSON diff of two files' metadata fields. |
| `fm_last_error()` / `fm_last_error_code()` | Thread-local message and `FM_*` code for the last failure. |
| `fm_free_string(p)` | Free a returned string; `NULL` is a no-op. |

## Tests

The test suite builds every fixture byte-by-byte in code (no binary samples): a PNG with a text chunk, a JPEG with a hand-built EXIF block in each byte order, an animated GIF, a WAV, an MP3 with ID3v2, ID3v1, and a real MPEG frame header, a FLAC with STREAMINFO and a comment, an MP4 with `mvhd`/`tkhd`, a PDF, a WebP, a BMP, text, and a minimal MKV — plus corrupt and truncated variants of several formats, the ID3v1 and PNG text write round-trips (including checking every PNG chunk CRC in the written file and that the `.bak` copy equals the original), compare diff detection, and graceful handling of an unsupported extension.

## Limitations

- **PDF page count is an approximation.** It counts page objects (`/Type /Page`, excluding `/Pages`) found in the raw file bytes. Compressed object streams can hide page objects, so large or optimised PDFs can undercount. This is stated here rather than hidden.
- **MP3 duration is an estimate.** It assumes a constant bitrate taken from the first frame header and divides the audio byte count by it; variable-bitrate files will be off.
- **Metadata editing is deliberately narrow.** Only MP3 ID3v1 and PNG `tEXt` writes are implemented — those are the formats where a safe, well-understood in-place edit exists. ID3v2 rewriting, WAV INFO chunks, and everything else return `FM_ERR_NOT_SUPPORTED`. ID3v1 text is Latin-1 and length-limited by the format (30 bytes for most fields, 4 for the year).
- **GIF frame count** comes from walking image descriptors; a malformed GIF stops the walk and reports `parse_error`.
- **MKV/WebM support is minimal:** document type and, when present inside the Info element, duration. No track or tag details.
- **EXIF coverage is four fields** (make, model, date/time, orientation) from the first IFD; GPS and thumbnail IFDs are not parsed.
- Timestamps (`modified_unix`) come from the filesystem and are 0 when the platform does not provide them.

## License

MIT — see `LICENSE`.
