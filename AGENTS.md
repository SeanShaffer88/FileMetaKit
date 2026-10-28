# FileMetaKit — agent notes

- Every parser in `src/meta.rs` is bounds-checked by construction: byte access goes through `Option`-returning helpers or `slice::get`. A parser that can panic on a truncated file is a bug, not a style issue — corrupt-file tests must keep passing.
- Reads never fail for content reasons: unsupported extensions return `"supported":false` + `reason`, and corrupt supported files return `"parse_error"` — both as JSON. NULL from `fm_read` means bad argument, missing file, or I/O error only.
- Writes are narrow on purpose: MP3 ID3v1 and PNG `tEXt` only. Do not widen the write surface without a format whose edit is provably safe in place. Every write keeps a `<path>.bak`, stages through a temp file in the same directory, and renames over the original.
- PNG `tEXt` writes must keep correct CRCs — the round-trip test recomputes every chunk CRC in the written file; do not weaken it.
- PDF page count and MP3 duration are documented approximations (see README Limitations). Do not present either as exact anywhere in docs or output field names.
- Zero dependencies; the compact JSON parser in `src/json.rs` rejects duplicate keys by design (family rule).
- Gate: WinDevKit `tools/gate.sh` (fmt + clippy both targets + full tests). Check/clippy do not link — CI's real Windows build is the link gate.
