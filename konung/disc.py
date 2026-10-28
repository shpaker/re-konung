"""Образы дисков Alcohol 120% (MDS + MDF): дата-трек в ISO, аудиодорожки в WAV.

Формат MDS описан в docs/disc-images.md.
"""

import struct
import wave
from dataclasses import dataclass
from pathlib import Path

RAW_SECTOR = 2352
# Образ может быть снят с субканалами: 96 байт после каждого сырого сектора
RAW_SECTOR_WITH_SUBCHANNEL = RAW_SECTOR + 96
# В сыром секторе Mode 1: 12 байт синхронизации, 4 байта заголовка, 2048 байт данных
MODE1_DATA_OFFSET = 16
MODE1_DATA_SIZE = 2048

# Режим дорожки — младшие 4 бита байта режима; старшие биты у разных
# программ записи различаются (0xAA и 0xEA — одно и то же)
MODE_AUDIO = 0x9
MODE_MODE1 = 0xA
MODE_MODE2_FORM1 = 0xC
# Mode 2 Form 1 (CD-XA): ещё 8 байт подзаголовка перед данными
MODE2_FORM1_DATA_OFFSET = 24

TRACK_BLOCK_SIZE = 80


@dataclass(frozen=True)
class Track:
    number: int
    mode: int
    sector_size: int
    start_sector: int
    file_offset: int
    pregap: int
    length: int

    @property
    def is_audio(self) -> bool:
        return self.mode & 0xF == MODE_AUDIO

    @property
    def data_offset(self) -> int | None:
        """Смещение 2048 байт данных в сыром секторе; None — не дата-трек."""
        return {
            MODE_MODE1: MODE1_DATA_OFFSET,
            MODE_MODE2_FORM1: MODE2_FORM1_DATA_OFFSET,
        }.get(self.mode & 0xF)


def read_tracks(mds_path: Path) -> list[Track]:
    """Список дорожек первой сессии из файла MDS."""
    data = mds_path.read_bytes()
    if not data.startswith(b"MEDIA DESCRIPTOR"):
        raise ValueError(f"{mds_path}: нет сигнатуры MEDIA DESCRIPTOR")

    session_offset = struct.unpack_from("<I", data, 0x50)[0]
    block_count = data[session_offset + 10]
    tracks_offset = struct.unpack_from("<I", data, session_offset + 20)[0]

    tracks = []
    for index in range(block_count):
        block = tracks_offset + index * TRACK_BLOCK_SIZE
        point = data[block + 4]
        # Точки 0xA0–0xA2 — служебные записи TOC, а не дорожки
        if not 1 <= point <= 99:
            continue
        extra_offset, sector_size = struct.unpack_from("<IH", data, block + 0x0C)
        start_sector, file_offset = struct.unpack_from("<IQ", data, block + 0x24)
        pregap, length = struct.unpack_from("<II", data, extra_offset)
        tracks.append(
            Track(
                number=point,
                mode=data[block],
                sector_size=sector_size,
                start_sector=start_sector,
                file_offset=file_offset,
                pregap=pregap,
                length=length,
            )
        )
    return tracks


def extract(mds_path: Path, out_dir: Path) -> list[Path]:
    """Пишет дата-трек в track01.iso, аудиодорожки — в trackNN.wav."""
    mdf_path = mds_path.with_suffix(".mdf")
    if not mdf_path.exists():
        mdf_path = next(mds_path.parent.glob("*.[mM][dD][fF]"))
    out_dir.mkdir(parents=True, exist_ok=True)

    written = []
    with mdf_path.open("rb") as mdf:
        for track in read_tracks(mds_path):
            if track.sector_size not in (RAW_SECTOR, RAW_SECTOR_WITH_SUBCHANNEL):
                raise ValueError(f"дорожка {track.number}: сектор {track.sector_size} байт")
            mdf.seek(track.file_offset)
            if track.is_audio:
                path = out_dir / f"track{track.number:02d}.wav"
                with wave.open(str(path), "wb") as wav:
                    wav.setnchannels(2)
                    wav.setsampwidth(2)
                    wav.setframerate(44100)
                    for _ in range(track.length):
                        wav.writeframes(mdf.read(track.sector_size)[:RAW_SECTOR])
            elif (offset := track.data_offset) is not None:
                path = out_dir / f"track{track.number:02d}.iso"
                with path.open("wb") as iso:
                    for _ in range(track.length):
                        sector = mdf.read(track.sector_size)
                        iso.write(sector[offset : offset + MODE1_DATA_SIZE])
            else:
                raise ValueError(f"дорожка {track.number}: неизвестный режим {track.mode:#x}")
            written.append(path)
    return written
