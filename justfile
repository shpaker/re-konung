set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

default:
    @just --list

# Шимы prek: pre-commit, commit-msg, pre-push. Повторный запуск ничего не меняет
setup:
    uv sync
    prek install --allow-missing-config

# Все проверки файлов, как перед push (без проверки сообщений коммитов)
check:
    prek run --all-files --stage manual

test:
    uv run pytest

lint:
    uv run ruff check
    uv run ruff format --check

fmt:
    uv run ruff format
    uv run ruff check --fix

# Rust-крейты в crates/: тесты и линты (cargo fmt, clippy)
cargo-test:
    cargo test --manifest-path crates/Cargo.toml

cargo-lint:
    cargo fmt --manifest-path crates/Cargo.toml --all --check
    cargo clippy --manifest-path crates/Cargo.toml --all-targets -- -D warnings

# Каталог установленной игры для запуска (original/k1/<издание>/game, docs/k1/editions.md);
# переопределяется переменной окружения: GAME_DIR=original/k1/eng/game just viewer 17
game_dir := env_var_or_default("GAME_DIR", "original/k1/rus/game")

# Просмотрщик карт: just viewer [N] [source] — карта N (без N — первая), источник game|tutorial
# dev-runtime + dynamic: Bevy in bevy_dylib.dll, binary link ~1 s (docs/rust.md, "Build profiles")
viewer map="" source="game":
    cargo run --profile dev-runtime --manifest-path crates/Cargo.toml -p kn-viewer --features kn-fe-viewer/dynamic -- --game-dir "{{game_dir}}" --source "{{source}}" {{ if map == "" { "" } else { "--map " + map } }}

# То же в release: бенчмарки и диагностика, обычный локальный цикл — just viewer
viewer-release map="" source="game":
    cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- --game-dir "{{game_dir}}" --source "{{source}}" {{ if map == "" { "" } else { "--map " + map } }}

# Frame-time gate (#98): 60-second release runs on the reference maps 17 and
# 19, both editions, playback playing, day/night lighting on. One BENCH and
# one GATE line per run; the gate reads avg_ms and p95_ms against 33.3 ms.
# The editions' paths are the protocol's, not {{game_dir}}: the gate always
# compares both, see docs/rust.md, "Frame time gate"
viewer-gate:
    cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- --game-dir original/k1/rus/game --map 17 --benchmark-secs 60
    cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- --game-dir original/k1/rus/game --map 19 --benchmark-secs 60
    cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- --game-dir original/k1/eng/game --map 17 --benchmark-secs 60
    cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- --game-dir original/k1/eng/game --map 19 --benchmark-secs 60

# Headless-диагностика без окна и GPU: just check-maps [source] — все карты источника.
# Release: the parser is CPU-bound; checking every map in dev-runtime is ten times slower
check-maps source="game":
    cargo run --release --manifest-path crates/Cargo.toml -p kn-viewer -- --game-dir "{{game_dir}}" --source "{{source}}" check --all

# Игра: just game — заготовка re-konung на стартовой карте новой игры
game:
    cargo run --profile dev-runtime --manifest-path crates/Cargo.toml -p kn-player --features kn-fe-player/dynamic -- --game-dir "{{game_dir}}"

# То же в release: бенчмарки и диагностика, обычный локальный цикл — just game
game-release:
    cargo run --release --manifest-path crates/Cargo.toml -p kn-player -- --game-dir "{{game_dir}}"

# kn-ui showcase: just ui-showcase — design tokens, theme, bundled fonts, widget helpers
ui-showcase:
    cargo run --profile dev-runtime --manifest-path crates/Cargo.toml -p kn-ui --example showcase

# Расширение Tiled «konung» — в папку расширений Tiled; запущенный Tiled подхватит его сам
[windows]
tiled-install:
    $dest = Join-Path $env:LOCALAPPDATA 'Tiled/extensions/konung'; if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }; Copy-Item -Recurse tiled/konung $dest; Write-Output $dest

[unix]
tiled-install:
    #!/usr/bin/env bash
    set -euo pipefail
    case "$(uname)" in
        Darwin) dest="$HOME/Library/Preferences/Tiled/extensions/konung" ;;
        *) dest="${XDG_CONFIG_HOME:-$HOME/.config}/tiled/extensions/konung" ;;
    esac
    rm -rf "$dest"
    mkdir -p "$(dirname "$dest")"
    cp -r tiled/konung "$dest"
    echo "$dest"

# Свой образ диска «Князя» (MDS/MDF в original/k1/<издание>/image) → original/k1/<издание>/{disc,files,game}
[windows]
k1-unpack edition:
    uv run konung disc (Get-ChildItem 'original/k1/{{edition}}/image/*.mds').FullName 'original/k1/{{edition}}/disc'
    & 'C:/Program Files/7-Zip/7z.exe' x -y -o'original/k1/{{edition}}/files' 'original/k1/{{edition}}/disc/track01.iso' | Out-Null
    & 'C:/Program Files/7-Zip/7z.exe' x -y -o'original/k1/{{edition}}/game' 'original/k1/{{edition}}/files/inst/setup.dat' | Out-Null; if ($LASTEXITCODE -gt 2) { exit $LASTEXITCODE } else { exit 0 }

# Следующая версия по коммитам с последнего тега (git-cliff, cliff.toml);
# пусто — с последнего тега нет коммитов, которые попадают в релиз
[unix]
next-version:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "$(uvx git-cliff --unreleased --strip all 2>/dev/null)" ]; then
        uvx git-cliff --bumped-version 2>/dev/null
    fi

# Следующая версия (см. вариант для unix выше)
[windows]
next-version:
    $notes = uvx git-cliff --unreleased --strip all 2>$null; if ($notes) { uvx git-cliff --bumped-version 2>$null }

# Текст релиза последнего тега (запускается на коммите с тегом)
changelog:
    uvx git-cliff --latest --strip all

# Расширение Tiled для релиза: out/konung-tiled-<тег>.zip с папкой konung/ и файлом VERSION
[unix]
tiled-package tag:
    #!/usr/bin/env bash
    set -euo pipefail
    stage="$(mktemp -d)"
    cp -r tiled/konung "$stage/konung"
    echo "{{tag}}" > "$stage/konung/VERSION"
    mkdir -p out
    rm -f "out/konung-tiled-{{tag}}.zip"
    (cd "$stage" && zip -qr - konung) > "out/konung-tiled-{{tag}}.zip"
    rm -rf "$stage"
    echo "out/konung-tiled-{{tag}}.zip"

# Расширение Tiled для релиза (см. вариант для unix выше)
[windows]
tiled-package tag:
    $stage = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid()); New-Item -ItemType Directory $stage | Out-Null; Copy-Item -Recurse tiled/konung (Join-Path $stage 'konung'); Set-Content -Encoding ascii (Join-Path $stage 'konung/VERSION') '{{tag}}'; New-Item -ItemType Directory -Force out | Out-Null; Compress-Archive -Force -Path (Join-Path $stage 'konung') -DestinationPath 'out/konung-tiled-{{tag}}.zip'; Remove-Item -Recurse -Force $stage; Write-Output 'out/konung-tiled-{{tag}}.zip'
