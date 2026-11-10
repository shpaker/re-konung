"""Issue and PR form check for CONTRIBUTING.md: English sections and a translation in <details>.

CI runs this on pull request descriptions (`.github/workflows/pr.yml`); the same functions validate
the repository's own templates (`tests/test_contrib_form.py`). Standard library only, so the check
runs in CI and in tests without a single dependency.
"""

from __future__ import annotations

import os
import re
import sys
from collections.abc import Sequence

# Mandatory sections: a `##` heading, in this order.
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

# An issue title starts with one of these and names the expected result rather than the action
# (CONTRIBUTING.md, "Issue titles: type and result"). Conventional Commits types belong to pull
# request titles only: git-cliff reads those, issue prefixes affect no release.
ISSUE_PREFIXES = ("bug", "feat", "task", "research")
ISSUE_TITLE = re.compile(rf"^(?:{'|'.join(ISSUE_PREFIXES)}): \S")

COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)
# Windows line endings are tolerated: bodies edited by Windows agents or UI often arrive
# with CRLF, and a `\r` before the line end would hide every heading and break the
# translation anchor. Normalizing first keeps the rest of the patterns plain.
CRLF = re.compile(r"\r\n?")
HEADINGS = re.compile(r"^##(?!#)[ \t]+(.+?)[ \t]*$", re.MULTILINE)
DETAILS_OPEN = re.compile(r"<details\b[^>]*>", re.IGNORECASE)
TRANSLATION_BLOCK = re.compile(
    r"<details\b[^>]*>\s*<summary>\s*(?P<summary>.*?)\s*</summary>(?P<text>.*?)</details>\s*$",
    re.DOTALL | re.IGNORECASE,
)
# Markup is searched for in a copy of the text where code is replaced by spaces of the same length:
# a quoted `<details>` or `## What` must not move the boundary of the English part.
CODE = re.compile(r"```.*?```|`[^`\n]*`", re.DOTALL)
# Forbidden mentions (AGENTS.md, "Git"). A path such as .claude/skills is not a mention: the name is
# preceded by a dot or a slash.
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
    """The same text with code turned into spaces: offsets match, markup inside code is gone."""
    return CODE.sub(lambda match: " " * len(match.group()), text)


def normalize(text: str) -> str:
    """Line endings to `\n`: a CRLF body must pass the same checks as an LF one."""
    return CRLF.sub("\n", text)


def structure(body: str) -> list[tuple[str, str]]:
    """`##` sections before the translation block: (heading, content from the original text)."""
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
    """The section holds an answer, not just the template comment it shipped with."""
    return bool(COMMENT.sub("", text).strip())


def check_sections(found: Sequence[tuple[str, str]], required: Sequence[str]) -> list[str]:
    """Findings about the structure: a missing section, an empty one, a broken order."""
    titles = [title for title, _ in found]
    problems = [f'section "## {title}" is missing' for title in required if title not in titles]
    if [title for title in titles if title in required] != [t for t in required if t in titles]:
        problems.append("mandatory sections are out of order: " + " → ".join(required))
    problems += [
        f'section "## {title}" is empty'
        for title, content in found
        if title in required and not has_content(content)
    ]
    return problems


def check_translation(body: str) -> list[str]:
    """The translation is the only collapsed block, last, with the right summary and not empty."""
    masked = mask_code(body)
    openings = list(DETAILS_OPEN.finditer(masked))
    if not openings:
        return [f'the description ends without a collapsed "{TRANSLATION_SUMMARY}" block']
    if len(openings) > 1:
        return [f"{len(openings)} collapsed blocks, expected one — the translation at the end"]
    block = TRANSLATION_BLOCK.search(masked)
    if block is None or block.start() != openings[0].start():
        return [
            "the translation must be the last block of the description: "
            f"<details><summary>{TRANSLATION_SUMMARY}</summary> … </details>, nothing after it"
        ]
    problems = []
    if block["summary"] != TRANSLATION_SUMMARY:
        problems.append(
            f"the collapsed block has <summary>{block['summary']}</summary>, "
            f"expected <summary>{TRANSLATION_SUMMARY}</summary>"
        )
    if not has_content(body[block.start("text") : block.end("text")]):
        problems.append("the translation block is empty")
    return problems


def check_body(body: str, required: Sequence[str]) -> list[str]:
    """Findings about a description: the structure of the English part and the translation block."""
    return check_translation(body) + check_sections(structure(body), required)


def check_issue_title(title: str) -> list[str]:
    """An issue title starts with one of ISSUE_PREFIXES; CI does not check it, the tests do."""
    hint = f"the issue title has no type prefix, expected one of: {', '.join(ISSUE_PREFIXES)}: ..."
    return [] if ISSUE_TITLE.match(title) else [hint]


def check_ai_mention(title: str, body: str) -> list[str]:
    hint = "the title or description holds a forbidden mention — see AGENTS.md, the Git section"
    return [] if AI_MENTION.search(f"{title}\n{body}") is None else [hint]


def check_pr(title: str, body: str) -> list[str]:
    """Every finding about a pull request: mentions and the shape of the description."""
    body = normalize(body)
    return check_ai_mention(title, body) + check_body(body, PR_SECTIONS)


def main() -> int:
    problems = check_pr(os.environ.get("PR_TITLE", ""), os.environ.get("PR_BODY", ""))
    if not problems:
        print("the PR description follows CONTRIBUTING.md")
        return 0
    print("the PR does not follow CONTRIBUTING.md:")
    for problem in problems:
        print(f"- {problem}")
    return 1


if __name__ == "__main__":
    sys.exit(main())
