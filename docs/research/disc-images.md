# MDS/MDF disc images

An image of the owner's disc consists of `*.mds` track metadata and `*.mdf` raw
sectors (Alcohol 120%). 7-Zip does not open this MDF representation; the
repository's `konung disc` (`konung/disc.py`) extracts tracks. This is the
supported extraction path, not an image distributor. Commands:
[prepare game files](../guides/game-files.md).

## MDS

Little-endian. **Confirmed** on RUS and ENG against the public libmirage format
description and our own images.

| Offset | Type | Meaning |
|---|---|---|
| 0x00 | 16 bytes | `MEDIA DESCRIPTOR` |
| 0x50 | u32 | First-session block offset |

Session block, 24 bytes: +10 is a u8 track-entry count including service records;
+20 is a u32 track-table offset. Each track record occupies 80 bytes:

| Offset | Type | Meaning |
|---|---|---|
| +0 | u8 | Low nibble: 9 audio, A Mode 1, C Mode 2 Form 1; programs differ in high bits (`0xAA`, `0xEC`) |
| +4 | u8 | Track number; 0xA0–0xA2 are service TOC records |
| +0x0C | u32 | Extra-block offset: u32 pregap followed by u32 sector count |
| +0x10 | u16 | MDF sector size: 2352, or 2448 with subchannels |
| +0x24 | u32 | First sector |
| +0x28 | u64 | MDF track offset |

Each data sector contributes 2048 bytes, starting at byte 16 for Mode 1 or byte
24 for Mode 2 Form 1. CD audio is 44100 Hz, 16-bit stereo.

## Checked editions

| Property | RUS | ENG |
|---|---|---|
| Sector size | 2352 | 2448, with subchannels |
| Data track | Mode 1, 212352 sectors | Mode 2 Form 1 (CD-XA), 162388 sectors |
| Audio | 10 tracks | 10 tracks |
