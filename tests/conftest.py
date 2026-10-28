"""Тесты на реальных файлах игры: оригиналы лежат в original/ (в git их нет).

Без них тесты на данных пропускаются, синтетические идут всегда.
Как получить original/ — docs/k1/editions.md.
"""

from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
K1 = ROOT / "original" / "k1"


def _need(path: Path) -> Path:
    if not path.exists():
        pytest.skip(f"нет {path.relative_to(ROOT)}: распакуйте оригинал (docs/k1/editions.md)")
    return path


@pytest.fixture(params=["rus", "eng"])
def k1_game(request) -> Path:
    """Папка установленной игры — содержимое inst/setup.dat."""
    return _need(K1 / request.param / "game")


@pytest.fixture
def k1_rus_image() -> Path:
    return _need(K1 / "rus" / "image")


@pytest.fixture
def k1_file(k1_game):
    """Файл игры по имени без учёта регистра: в дистрибутиве регистр разный."""

    def find(name: str) -> Path:
        return next(p for p in k1_game.iterdir() if p.name.lower() == name)

    return find
