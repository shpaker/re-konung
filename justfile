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

# Образ диска «Князя» (RAR с MDS/MDF из original/k1/rar) → original/k1/<издание>/{image,disc,files,game}
[windows]
k1-unpack edition archive:
    & 'C:/Program Files/7-Zip/7z.exe' x -y -o'original/k1/{{edition}}/image' '{{archive}}' | Out-Null
    uv run konung disc (Get-ChildItem 'original/k1/{{edition}}/image/*.mds').FullName 'original/k1/{{edition}}/disc'
    & 'C:/Program Files/7-Zip/7z.exe' x -y -o'original/k1/{{edition}}/files' 'original/k1/{{edition}}/disc/track01.iso' | Out-Null
    & 'C:/Program Files/7-Zip/7z.exe' x -y -o'original/k1/{{edition}}/game' 'original/k1/{{edition}}/files/inst/setup.dat' | Out-Null; if ($LASTEXITCODE -gt 2) { exit $LASTEXITCODE }

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
