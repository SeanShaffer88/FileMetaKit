# FileMetaKit Roadmap

## v0.1.0 (current)

A single read / edit / compare API over common formats, all parsers hand-written and bounds-checked with zero dependencies: PNG, JPEG + EXIF, GIF, BMP, WebP, WAV, MP3 (ID3v2 + ID3v1 + first-frame header), FLAC, Ogg Vorbis, MP4/MOV, MKV/WebM (minimal), PDF, and text. Writes for MP3 ID3v1 tags and PNG `tEXt` chunks, each with a `.bak` backup and a staged rename.

## Next, in order

1. **More EXIF coverage**: exposure, focal length, and GPS coordinates from the EXIF sub-IFD, still bounds-checked and still returning partial results rather than failing.
2. **TIFF and JPEG 2000 / AVIF / HEIC headers**: dimension-only support first, following the WebP pattern.
3. **MP3 Xing/LAME header parsing** so variable-bitrate durations come from the header's frame count instead of the constant-bitrate estimate.
4. **PDF metadata from the XMP packet** as a supplement to the Info dictionary, and a page count that can follow compressed object streams if it can be done without a full PDF parser.
5. **ID3v2 writing**, only if it can preserve unknown frames byte-for-byte and pass a round-trip test as strict as the PNG CRC test.

## Explicitly not planned

- Full metadata editing for every format: most container formats make in-place edits unsafe; the narrow write surface is the design, not a gap.
- Thumbnail generation or decoding any image/audio/video data — this kit reads headers and tags only.
