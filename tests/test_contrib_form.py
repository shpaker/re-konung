"""Форма Issues и PR по CONTRIBUTING.md: разделы, перевод, запрещённые упоминания.

Тот же код CI запускает на каждом pull request (`.github/scripts/contrib_form.py`); здесь им
проверяются и шаблоны репозитория, и тексты документации, чтобы форма не разъехалась.
"""

import importlib.util
import sys
from pathlib import Path
from typing import Any

import pytest

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / ".github" / "scripts" / "contrib_form.py"
GITHUB = ROOT / ".github"


def _load(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


form = _load("contrib_form", SCRIPT)

# Ответ каждого обязательного раздела — достаточно любого, чтобы раздел считался заполненным.
BODY = {
    "What": "The map parser keeps the unknown bytes instead of dropping them.",
    "Why": "The round-trip test could not prove the writer is byte-exact.",
    "Related": "- #34",
    "How to Test": "`uv run pytest tests/k1/test_maps.py`",
    "Review Guide": "Start at `konung/k1/map.py`, then read the round-trip test.",
    "Risks": "None",
    "Environment": "macOS, Python 3.14, edition RUS.",
}

TRANSLATION = """<details>
<summary>Русский перевод</summary>

## Что

Парсер карт сохраняет неизвестные байты, а не выкидывает их.

## Зачем

Тест на побайтовый возврат не доказывал, что записывающий код точен.

## Имеет отношение к

- #34

## Как проверить

`uv run pytest tests/k1/test_maps.py`

## Как делать ревью

Начать с `konung/k1/map.py`, затем тест на побайтовый возврат.

## Риски

None

## Окружение

macOS, Python 3.14, издание RUS.

</details>"""


def render(**overrides: str | None) -> str:
    """Тело PR: разделы в порядке PR_SECTIONS; None — раздела в описании нет вовсе."""
    sections = {title: BODY[title] for title in form.PR_SECTIONS}
    sections.update(overrides)
    parts = [f"## {title}\n\n{value}" for title, value in sections.items() if value is not None]
    return "\n\n".join(parts) + f"\n\n{TRANSLATION}\n"


def problems(**overrides: str | None) -> list[str]:
    return form.check_body(render(**overrides), form.PR_SECTIONS)


def test_correct_body_passes():
    assert form.check_pr("fix: keep the unknown map bytes", render()) == []


def test_missing_section_is_named():
    assert problems(Risks=None) == ["нет раздела «## Risks»"]


def test_empty_section_is_named():
    assert problems(Risks="")[0] == "раздел «## Risks» пуст"


def test_section_with_only_a_comment_is_empty():
    assert "раздел «## What» пуст" in problems(What="<!-- TODO: what changed -->")


@pytest.mark.parametrize("answer", ["None", "none", "N/A", "n/a", "-", "—"])
def test_not_applicable_answer_is_an_answer(answer: str):
    assert problems(Risks=answer) == []


def test_sections_out_of_order_are_named():
    body = (
        render()
        .replace("## Risks", "## TMP")
        .replace("## Environment", "## Risks")
        .replace("## TMP", "## Environment")
    )
    assert any("не в порядке" in problem for problem in form.check_body(body, form.PR_SECTIONS))


def test_headings_inside_the_translation_do_not_count():
    """Заголовки внутри <details> не обходят проверку: разделы ищутся только до блока."""
    english = "Prose with no headings at all.\n\n"
    inside = "\n\n".join(f"## {title}\n\n{BODY[title]}" for title in form.PR_SECTIONS)
    body = english + f"<details>\n<summary>Русский перевод</summary>\n\n{inside}\n</details>\n"
    missing = [p for p in form.check_body(body, form.PR_SECTIONS) if "нет раздела" in p]
    assert len(missing) == len(form.PR_SECTIONS)


def test_body_without_translation_is_named():
    body = render().replace(TRANSLATION, "")
    assert any(
        "нет сворачиваемого блока" in problem for problem in form.check_body(body, form.PR_SECTIONS)
    )


def test_translation_summary_must_be_the_russian_one():
    body = render().replace("<summary>Русский перевод</summary>", "<summary>Перевод</summary>")
    assert any(
        "<summary>Перевод</summary>" in problem
        for problem in form.check_body(body, form.PR_SECTIONS)
    )


def test_empty_translation_is_named():
    body = render().replace(
        TRANSLATION, "<details>\n<summary>Русский перевод</summary>\n</details>\n"
    )
    assert form.check_body(body, form.PR_SECTIONS) == ["блок перевода пуст"]


def test_text_after_the_translation_is_named():
    body = render() + "Запасной раздел после перевода.\n"
    assert any("должен быть последним" in p for p in form.check_body(body, form.PR_SECTIONS))


def test_second_details_block_is_named():
    body = render().replace(
        TRANSLATION,
        "<details>\n<summary>Дамп</summary>\n\n`uv run pytest`\n\n</details>\n\n" + TRANSLATION,
    )
    assert form.check_body(body, form.PR_SECTIONS) == [
        "сворачиваемых блоков 2, а должен быть один — перевод в конце описания"
    ]


@pytest.mark.parametrize(
    "text",
    [
        "Co-authored-by: someone <a@example.com>",
        "Generated with a helper",
        "упоминание qwen в тексте",
        "Запрещённое ИИ в описании",
        "нейросети писали этот код",
        "ветка ai/whatever",
    ],
)
def test_forbidden_mention_is_named(text: str):
    assert form.check_ai_mention("fix: keep the unknown map bytes", text) != []


def test_paths_are_not_mentions():
    body = "Работаем над `.claude/skills/konung-reverse-format/SKILL.md` и `skills/` в корне."
    assert form.check_ai_mention("docs: describe the plugin skills", body) == []


@pytest.mark.parametrize(
    ("template", "required"),
    [
        ("pull_request_template.md", form.PR_SECTIONS),
        ("ISSUE_TEMPLATE/bug_report.md", form.BUG_SECTIONS),
        ("ISSUE_TEMPLATE/feature_request.md", form.FEATURE_SECTIONS),
    ],
)
def test_repo_template_has_the_sections_in_order(template: str, required: tuple[str, ...]):
    text = (GITHUB / template).read_text(encoding="utf-8")
    titles = [title for title, _ in form.headings(form.english_part(text))]
    assert titles == list(required)


@pytest.mark.parametrize(
    "template",
    [
        "pull_request_template.md",
        "ISSUE_TEMPLATE/bug_report.md",
        "ISSUE_TEMPLATE/feature_request.md",
    ],
)
def test_repo_template_has_a_translation_block(template: str):
    text = (GITHUB / template).read_text(encoding="utf-8")
    assert form.check_translation(text) == []


def test_issue_templates_need_a_choice():
    config = (GITHUB / "ISSUE_TEMPLATE" / "config.yml").read_text(encoding="utf-8")
    assert "blank_issues_enabled: false" in config


def test_contributing_documents_the_same_sections():
    text = (ROOT / "CONTRIBUTING.md").read_text(encoding="utf-8")
    for section in (*form.PR_SECTIONS, *form.BUG_SECTIONS, *form.FEATURE_SECTIONS):
        assert f"`{section}`" in text or f"## {section}" in text, section
