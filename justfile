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
