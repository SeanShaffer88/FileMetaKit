# FileMetaKit — C ABI / Rust API parity

Scored from the code at version 0.1.0. The C ABI is a thin wrapper: every export calls the same Rust library function, so the two surfaces cannot drift in behaviour.

| Capability | C ABI | Rust API | Notes |
|---|---|---|---|
| Read metadata | Yes (`fm_read`) | Yes (`meta::read_file_json`, `meta::read_metadata_bytes`) | Same `Meta::to_json` output. |
| Edit metadata | Yes (`fm_write`) | Yes (`meta::write_file_json`) | Same supported set: MP3 ID3v1, PNG `tEXt`. |
| Compare metadata | Yes (`fm_compare`) | Yes (`meta::compare_file_json`, `meta::compare_meta`) | Same field-diff logic. |
| Version | Yes (`fm_version`) | Yes (`VERSION` constants) | Same source value. |
| Errors | Codes + thread-local message (`fm_last_error`, `fm_last_error_code`) | `(code, message)` in the `Result` error | The FFI layer copies the Rust error into the thread-local slot. |
| Freeing results | Yes (`fm_free_string`) | n/a | Rust callers own a `String`; C callers own a `char*`. |

**Parity summary:** 100% of the core job (read, edit, compare) is available in both surfaces with identical results, because both call one implementation.
