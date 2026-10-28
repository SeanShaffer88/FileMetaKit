/* FileMetaKit.dll — C ABI. All strings UTF-8, NUL-terminated.
 * char* results are caller-owned: free with fm_free_string().
 * On failure they return NULL; see fm_last_error() / fm_last_error_code().
 *
 * Unsupported files are NOT a failure at fm_read time: the returned JSON
 * carries "supported":false plus a "reason". Corrupt/truncated supported
 * files return JSON with a "parse_error" field. NULL is reserved for
 * bad arguments, missing files, and I/O errors. */
#ifndef FILEMETAKIT_H
#define FILEMETAKIT_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#if defined(_WIN32) && defined(FM_BUILD)
#define FM_API __declspec(dllexport)
#elif defined(_WIN32)
#define FM_API __declspec(dllimport)
#else
#define FM_API
#endif
#define FM_OK 0
#define FM_ERR_INVALID_ARG 1
#define FM_ERR_NOT_SUPPORTED 2
#define FM_ERR_NOT_FOUND 3
#define FM_ERR_ACCESS_DENIED 4
#define FM_ERR_PLATFORM 5
#define FM_ERR_INTERNAL 6

/* Library version string, e.g. "0.1.0". Static storage — do NOT free. */
FM_API const char *fm_version(void);

/* Thread-local message for the most recent failure on this thread.
 * Static/thread storage — do NOT free. Empty when there is no error. */
FM_API const char *fm_last_error(void);

/* FM_* code for the most recent failure on this thread (FM_OK if none). */
FM_API int fm_last_error_code(void);

/* Free a caller-owned char* returned by fm_read/fm_write/fm_compare.
 * Passing NULL is a no-op. */
FM_API void fm_free_string(char *p);

/* Read metadata for the file at `path` (UTF-8).
 * Returns caller-owned JSON:
 *   {"file_name":..., "extension":..., "size":..., "modified_unix":...,
 *    "kind":"image|audio|video|document|other", "mime":...,
 *    "supported":true|false, ["reason":...,] ["parse_error":...,]
 *    "fields":{ ... format-specific fields ... }}
 * Formats: PNG (width,height,bit_depth,color_type, tEXt entries),
 * JPEG (width,height, EXIF make/model/datetime/orientation),
 * GIF (width,height,frames,animated), BMP (width,height,bpp),
 * WebP (width,height), WAV (duration_sec,sample_rate,channels,bits),
 * MP3 (ID3v2/ID3v1 title,artist,album,year,track,genre, plus
 * bitrate_kbps,sample_rate,duration_sec estimate),
 * FLAC (sample_rate,channels,duration_sec, Vorbis comment tags),
 * OGG Vorbis (sample_rate,channels, comment tags),
 * MP4/MOV (duration_sec,timescale,duration_units,width,height),
 * MKV/WebM (doc_type, duration_sec when present),
 * PDF (version,pages approximation,title,author when plainly present),
 * TXT/MD (lines,words,encoding).
 * Returns NULL + FM_ERR_NOT_FOUND / FM_ERR_INVALID_ARG on failure. */
FM_API char *fm_read(const char *path);

/* Apply edits given as a JSON object of string/number values, e.g.
 *   fm_write("song.mp3", "{\"title\":\"New\",\"track\":3}")
 * Only two write targets are implemented:
 *   - MP3: an ID3v1 tag is written/replaced in place (title, artist,
 *     album, year, comment, track, genre as a name or 0-255 number).
 *   - PNG: tEXt chunks with the given keywords are inserted/replaced
 *     (Latin-1 values, correct CRCs) immediately before IEND.
 * Every other format or field returns NULL + FM_ERR_NOT_SUPPORTED with
 * the reason in fm_last_error(). Writes keep a "<path>.bak" copy of the
 * original next to the file, write a temp file in the same directory,
 * then rename it over the original.
 * On success returns caller-owned JSON:
 *   {"format":"id3v1|png_text","applied":{...},"backup":"<path>.bak"} */
FM_API char *fm_write(const char *path, const char *edits_json);

/* Compare the fm_read field sets of two files.
 * Returns caller-owned JSON:
 *   {"same_format":bool,
 *    "differences":[{"field":..., "a":<value|null>, "b":<value|null>}],
 *    "identical_metadata":bool}
 * `same_format` compares the (lowercased) file extensions.
 * Returns NULL on bad arguments / missing files (see fm_last_error()). */
FM_API char *fm_compare(const char *path_a, const char *path_b);

#ifdef __cplusplus
}
#endif
#endif
