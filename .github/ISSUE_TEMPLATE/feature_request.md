---
name: Feature request
about: A task the project should deliver — behaviour, a format, a tool
title: 'feat: '
labels: enhancement
assignees: ''
---

<!--
Заголовок начинается с префикса типа и называет ожидаемый результат, а не действие: `feat:` —
новый наблюдаемый результат, `task:` — работа без него, `research:` — результат это факт в
`docs/`; `bug:` несёт шаблон Bug report. Скоуп вроде `(kn-io)` не обязателен — слои перечислены
в Scope.

Дальше: заголовок и всё описание — на английском, перевод — последним сворачиваемым блоком
(заготовки ниже). Один тикет — одна самостоятельная задача: тикет описывает проблему и
ожидаемый результат, а не предписывает реализацию. Обязательные разделы не оставляют
пустыми: неприменимое пишут как `None` или `N/A`. Необязательные — Implementation Notes,
Alternatives, Dependencies / Related Issues — дописывают только когда есть что сказать:
подтверждённые факты со ссылками и открытые вопросы идут в Implementation Notes.
Зависимость перечисляют в Dependencies номерами, а настоящую связь ставит панель
«Relationships» или `gh issue edit <n> --add-blocked-by <m>`: текст сам ничего не
блокирует. Тикет закрывает PR строкой `Closes #NN` в своём разделе `Related`.
-->

## Problem

<!-- What is missing or wrong today and why it matters. The problem, not the implementation. -->

## Proposed Behavior

<!--
What a reviewer can observe or run once the ticket is done. Separate confirmed original
behaviour from a hypothesis and say which is which: an unconfirmed rule stays
unconfirmed and is not replaced by an invented one.
-->

## Acceptance Criteria

<!--
Binary checks, one per line: the command to run and what its output must show.
"Works correctly" is not a criterion.
-->

## Scope

<!-- What this ticket covers and what it deliberately leaves out. -->

<details>
<summary>Русский перевод</summary>

## Проблема

## Предлагаемое поведение

## Критерии приёмки

## Область

</details>
