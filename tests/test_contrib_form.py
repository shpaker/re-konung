"""Issue and PR form per CONTRIBUTING.md: sections, translation, forbidden mentions.

CI runs the same code on every pull request (`.github/scripts/contrib_form.py`); here it also
validates the repository's own templates and the rule texts, so the form cannot drift apart.
"""

import importlib.util
import re
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

# One answer per mandatory section — any of them is enough for the section to count as filled.
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
    """A PR body with sections in PR_SECTIONS order; None drops the section entirely."""
    sections = {title: BODY[title] for title in form.PR_SECTIONS}
    sections.update(overrides)
    parts = [f"## {title}\n\n{value}" for title, value in sections.items() if value is not None]
    return "\n\n".join(parts) + f"\n\n{TRANSLATION}\n"


def problems(**overrides: str | None) -> list[str]:
    return form.check_body(render(**overrides), form.PR_SECTIONS)


def test_correct_body_passes():
    assert form.check_pr("fix: keep the unknown map bytes", render()) == []


def test_missing_section_is_named():
    assert problems(Risks=None) == ['section "## Risks" is missing']


def test_empty_section_is_named():
    assert problems(Risks="")[0] == 'section "## Risks" is empty'


def test_section_with_only_a_comment_is_empty():
    assert 'section "## What" is empty' in problems(What="<!-- TODO: what changed -->")


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
    assert any("out of order" in problem for problem in form.check_body(body, form.PR_SECTIONS))


def test_headings_inside_the_translation_do_not_count():
    """Headings in <details> cannot get past the check: sections are read only before the block."""
    english = "Prose with no headings at all.\n\n"
    inside = "\n\n".join(f"## {title}\n\n{BODY[title]}" for title in form.PR_SECTIONS)
    body = english + f"<details>\n<summary>Русский перевод</summary>\n\n{inside}\n</details>\n"
    missing = [p for p in form.check_body(body, form.PR_SECTIONS) if "is missing" in p]
    assert len(missing) == len(form.PR_SECTIONS)


def test_body_without_translation_is_named():
    body = render().replace(TRANSLATION, "")
    assert any(
        "without a collapsed" in problem for problem in form.check_body(body, form.PR_SECTIONS)
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
    assert form.check_body(body, form.PR_SECTIONS) == ["the translation block is empty"]


def test_text_after_the_translation_is_named():
    body = render() + "Запасной раздел после перевода.\n"
    assert any("must be the last block" in p for p in form.check_body(body, form.PR_SECTIONS))


def test_second_details_block_is_named():
    body = render().replace(
        TRANSLATION,
        "<details>\n<summary>Дамп</summary>\n\n`uv run pytest`\n\n</details>\n\n" + TRANSLATION,
    )
    assert form.check_body(body, form.PR_SECTIONS) == [
        "2 collapsed blocks, expected one — the translation at the end"
    ]


def test_markup_quoted_in_code_is_not_markup():
    """The form's own text quotes `<details>` and `## What`: quoted markup is not markup."""
    quoted = (
        BODY["What"]
        + "\n\nThe template quotes `<details>` literally:\n\n"
        + "```markdown\n## What\n```\n\nand nothing else."
    )
    assert form.check_body(render(What=quoted), form.PR_SECTIONS) == []


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
    assert [title for title, _ in form.structure(text)] == list(required)


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


def test_descriptions_must_be_concise():
    """A ticket and a PR body are short sentences, not a retelling of the diff — in both docs."""
    rule = "Descriptions are concise"
    for name in ("CONTRIBUTING.md", "AGENTS.md"):
        assert rule in (ROOT / name).read_text(encoding="utf-8"), name


def test_github_linking_keywords_are_documented():
    """Only GitHub keywords make links work, so they have to stay in the rules."""
    contributing = (ROOT / "CONTRIBUTING.md").read_text(encoding="utf-8")
    assert "Closes #" in contributing and "--add-blocked-by" in contributing
    assert "Closes #" in (GITHUB / "pull_request_template.md").read_text(encoding="utf-8")
    feature = (GITHUB / "ISSUE_TEMPLATE/feature_request.md").read_text(encoding="utf-8")
    assert "--add-blocked-by" in feature
    bug = (GITHUB / "ISSUE_TEMPLATE/bug_report.md").read_text(encoding="utf-8")
    assert "do not invent steps" in bug


@pytest.mark.parametrize("prefix", form.ISSUE_PREFIXES)
def test_issue_title_needs_one_of_the_documented_prefixes(prefix: str):
    assert form.check_issue_title(f"{prefix}: creatures move along the original maps") == []
    assert f"`{prefix}:`" in (ROOT / "CONTRIBUTING.md").read_text(encoding="utf-8")


@pytest.mark.parametrize(
    "title",
    [
        "creatures move along the original maps",
        "perf: creatures render without rebuilding the scene",
        "chore: unify issue forms",
        "feat:",
        "bug:map does not open",
    ],
)
def test_issue_title_without_a_type_prefix_is_named(title: str):
    assert form.check_issue_title(title) != []


def test_templates_prefill_an_issue_prefix():
    for template, prefix in (
        ("ISSUE_TEMPLATE/bug_report.md", "bug"),
        ("ISSUE_TEMPLATE/feature_request.md", "feat"),
    ):
        text = (GITHUB / template).read_text(encoding="utf-8")
        match = re.search(r"(?m)^title: '(?P<value>[^']*)'$", text)
        assert match and match.group("value") == f"{prefix}: ", template


def test_issue_prefixes_stay_out_of_pull_request_titles():
    """An issue title carries a type prefix, a PR title keeps Conventional Commits."""
    contributing = (ROOT / "CONTRIBUTING.md").read_text(encoding="utf-8")
    assert "## Issue titles: type and result" in contributing
    section = contributing.split("## Issue titles: type and result", 1)[1].split("\n## ", 1)[0]
    assert "Conventional Commits" in section
    assert "task: implement pathfinding" in section


@pytest.mark.parametrize(
    "template",
    ["pull_request_template.md", "ISSUE_TEMPLATE/config.yml"]
    + [f"ISSUE_TEMPLATE/{name}" for name in ("bug_report.md", "feature_request.md")],
)
def test_templates_are_english_outside_the_translation(template: str):
    """Templates and their comments are English; Russian stays inside the translation block."""
    text = (GITHUB / template).read_text(encoding="utf-8")
    english = text[: text.lower().index("<details")] if "<details" in text.lower() else text
    assert not re.search(r"[А-Яа-яЁё]", english), template
