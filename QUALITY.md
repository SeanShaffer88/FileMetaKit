# FileMetaKit — Quality Scorecard

Version scored: 0.1.0 (2026-10-28). Scores are from evidence gathered on the build machine: the full gate output and the test suite. Windows DLL runtime behaviour is **Unverified** — the library has not yet been loaded by a Windows host program, so no score here claims it works there beyond the Windows-target check/clippy passes in the gate.

| Area | Score | Evidence |
|---|---|---|
| Core logic | 9/10 | All parsers are pure functions over byte slices in `src/meta.rs`, shared identically by the Rust API and the C ABI; every parser is bounds-checked and returns partial results or a `parse_error` instead of panicking. |
| Tests | 9/10 | Full gate run 2026-10-28 (see below): 21 tests with byte-built fixtures for PNG, JPEG+EXIF (both TIFF byte orders), GIF, WAV, MP3 (ID3v2+ID3v1+frame), FLAC, Ogg Vorbis, MP4, PDF, WebP, BMP, text, and MKV; corrupt/truncated cases; ID3v1 and PNG `tEXt` write round-trips with `.bak` and CRC checks; compare diff detection; unsupported-format handling. |
| Safety of user work | 9/10 | Writes are limited to two provably safe formats; each write keeps a `<path>.bak` original, stages through a temp file in the same directory, and renames over the original; unsupported writes are refused before the file is touched. |
| Documentation | 9/10 | README with TL;DR, a C example, the full API table, and honest limitations (PDF page-count and MP3 duration approximations stated plainly); the header documents every export. |
| Packaging | 6/10 | Standard WinDevKit crate layout (cdylib + rlib + staticlib, release profile) and `ci.yml` are in place, but no release has been built or published from this repo yet. |
| Windows runtime | Unverified | Not yet loaded on a real Windows machine. A host-program field test is required before this can be scored. |

**Overall: 42/50 on scored areas (84/100 on those areas), with Windows runtime Unverified.** The gate result recorded at scoring time: GATE GREEN (format, clippy host + Windows target, Windows-target check, full test suite).
