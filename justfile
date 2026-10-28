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
