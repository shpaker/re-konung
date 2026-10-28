"""Последовательное чтение little-endian полей из буфера."""

import struct


class Reader:
    def __init__(self, data: bytes, pos: int = 0) -> None:
        self.data = data
        self.pos = pos

    def __len__(self) -> int:
        return len(self.data)

    def _unpack(self, fmt: str):
        value = struct.unpack_from(fmt, self.data, self.pos)[0]
        self.pos += struct.calcsize(fmt)
        return value

    def u8(self) -> int:
        return self._unpack("<B")

    def s8(self) -> int:
        return self._unpack("<b")

    def u16(self) -> int:
        return self._unpack("<H")

    def s16(self) -> int:
        return self._unpack("<h")

    def u32(self) -> int:
        return self._unpack("<I")

    def s32(self) -> int:
        return self._unpack("<i")

    def bytes(self, count: int) -> bytes:
        if self.pos + count > len(self.data):
            raise EOFError(f"нужно {count} байт с {self.pos}, а в буфере {len(self.data)}")
        chunk = self.data[self.pos : self.pos + count]
        self.pos += count
        return chunk

    def skip(self, count: int) -> None:
        self.pos += count

    def seek(self, pos: int) -> None:
        self.pos = pos
