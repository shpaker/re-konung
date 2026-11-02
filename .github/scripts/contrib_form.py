"""Форма описаний Issues и PR по CONTRIBUTING.md: английские разделы и перевод в <details>.

CI проверяет этим описание pull request (`.github/workflows/pr.yml`); те же функции
проверяют шаблоны репозитория (`tests/test_contrib_form.py`). Только стандартная библиотека,
чтобы проверку можно было запускать и в CI, и в тестах без единой зависимости.
"""

from __future__ import annotations

import os
import re
import sys
from collections.abc import Sequence

# Обязательные разделы: заголовок уровня `##`, в этом порядке.
PR_SECTIONS = ("What", "Why", "Related", "How to Test", "Review Guide", "Risks", "Environment")
BUG_SECTIONS = (
    "Summary",
    "Steps to Reproduce",
    "Expected Behavior",
    "Actual Behavior",
    "Environment",
)
FEATURE_SECTIONS = ("Problem", "Proposed Behavior", "Acceptance Criteria", "Scope")

TRANSLATION_SUMMARY = "Русский перевод"

COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)
HEADINGS = re.compile(r"^##(?!#)[ \t]+(.+?)[ \t]*$", re.MULTILINE)
DETAILS_OPEN = re.compile(r"<details\b[^>]*>", re.IGNORECASE)
TRANSLATION_BLOCK = re.compile(
    r"<details\b[^>]*>\s*<summary>\s*(?P<summary>.*?)\s*</summary>(?P<text>.*?)</details>\s*$",
    re.DOTALL | re.IGNORECASE,
)
# Разметку ищем в тексте, где блоки кода заменены пробелами той же длины: цитата вроде
# `<details>` или `## What` из шаблона не должна двигать границу английской части.
CODE = re.compile(r"```.*?```|`[^`\n]*`", re.DOTALL)
# Запрещённые упоминания (AGENTS.md, раздел «Git»). Пути вроде .claude/skills — не упоминание:
# перед именем точка или слэш.
AI_MENTION = re.compile(
    r"(?<![./\w])(claude|qwen|anthropic|openai|chatgpt|copilot)\b"
    r"|co-authored-by"
    r"|generated with"
    r"|(?<!\w)ИИ(?!\w)"
    r"|нейросет"
    r"|\bai/",
    re.IGNORECASE,
)


def mask_code(text: str) -> str:
    """Тот же текст, но код — пробелы: смещения совпадают, разметка внутри кода исчезла."""
    return CODE.sub(lambda match: " " * len(match.group()), text)


def structure(body: str) -> list[tuple[str, str]]:
    """Разделы `##` до блока перевода: (заголовок, содержимое из исходного текста)."""
    masked = mask_code(body)
    opening = DETAILS_OPEN.search(masked)
    end = len(masked) if opening is None else opening.start()
    found = list(HEADINGS.finditer(masked[:end]))
    result = []
    for index, match in enumerate(found):
        stop = found[index + 1].start() if index + 1 < len(found) else end
        result.append((match.group(1), body[match.end() : stop]))
    return result


def has_content(text: str) -> bool:
    """В разделе есть ответ, а не только комментарий-заготовка из шаблона."""
    return bool(COMMENT.sub("", text).strip())


def check_sections(found: Sequence[tuple[str, str]], required: Sequence[str]) -> list[str]:
    """Замечания к структуре: нет раздела, раздел пуст, порядок нарушен."""
    titles = [title for title, _ in found]
    problems = [f"нет раздела «## {title}»" for title in required if title not in titles]
    if [title for title in titles if title in required] != [t for t in required if t in titles]:
        problems.append("обязательные разделы идут не в порядке: " + " → ".join(required))
    problems += [
        f"раздел «## {title}» пуст"
        for title, content in found
        if title in required and not has_content(content)
    ]
    return problems


def check_translation(body: str) -> list[str]:
    """Перевод — единственный сворачиваемый блок в конце, с нужным заголовком и не пустой."""
    masked = mask_code(body)
    openings = list(DETAILS_OPEN.finditer(masked))
    if not openings:
        return [f"в конце описания нет сворачиваемого блока «{TRANSLATION_SUMMARY}»"]
    if len(openings) > 1:
        return [
            f"сворачиваемых блоков {len(openings)}, а должен быть один — перевод в конце описания"
        ]
    block = TRANSLATION_BLOCK.search(masked)
    if block is None or block.start() != openings[0].start():
        return [
            "блок перевода должен быть последним в описании: "
            f"<details><summary>{TRANSLATION_SUMMARY}</summary> … </details>, после него ничего"
        ]
    problems = []
    if block["summary"] != TRANSLATION_SUMMARY:
        problems.append(
            f"в сворачиваемом блоке <summary>{block['summary']}</summary>, "
            f"нужно <summary>{TRANSLATION_SUMMARY}</summary>"
        )
    if not has_content(body[block.start("text") : block.end("text")]):
        problems.append("блок перевода пуст")
    return problems


def check_body(body: str, required: Sequence[str]) -> list[str]:
    """Замечания к описанию: структура английской части и блок перевода."""
    return check_translation(body) + check_sections(structure(body), required)


def check_ai_mention(title: str, body: str) -> list[str]:
    hint = "в заголовке или описании есть запрещённое упоминание — см. AGENTS.md, раздел «Git»"
    return [] if AI_MENTION.search(f"{title}\n{body}") is None else [hint]


def check_pr(title: str, body: str) -> list[str]:
    """Все замечания к pull request: упоминания и форма описания."""
    return check_ai_mention(title, body) + check_body(body, PR_SECTIONS)


def main() -> int:
    problems = check_pr(os.environ.get("PR_TITLE", ""), os.environ.get("PR_BODY", ""))
    if not problems:
        print("форма описания PR соответствует CONTRIBUTING.md")
        return 0
    print("PR не по CONTRIBUTING.md:")
    for problem in problems:
        print(f"- {problem}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
