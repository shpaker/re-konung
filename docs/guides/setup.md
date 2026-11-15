<a id="set-up-a-checkout"></a>

# Подготовка окружения

Установите [uv](https://docs.astral.sh/uv/), [just](https://just.systems) и
[prek](https://prek.j178.dev). Проект требует Python 3.14 или новее; uv выбирает
совместимый интерпретатор, версия 3.14 отдельно не закреплена. Для приложений
Rust нужен стабильный toolchain, для редактирования карт — Tiled 1.12.
Извлечение образа диска также требует 7-Zip.

Из корня репозитория:

```sh
uv tool install prek
just setup
```

Команды устанавливают зависимости Python и общие git-хуки. После изменений
выполняйте `just check`. Профили Rust, системные зависимости и CI описаны
в [сборке](../development/building.md), состав проверок —
в [проверках](../development/testing.md).

Подготовьте [файлы игры](game-files.md), затем выберите инструмент:

- [Игра](player.md): `just game`.
- [Просмотрщик](viewer.md): `just viewer 17`.
- [Tiled](tiled.md): `just tiled-install`.

Расширение Tiled можно установить и через плагин репозитория без рабочего
каталога разработки; этот способ описан в
[руководстве](tiled.md#installation-through-an-agent).
