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
