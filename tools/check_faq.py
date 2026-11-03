#!/usr/bin/env python3
"""Check the FAQ's format and references.

A `#` title and a preamble, then `##` topic headings, each holding `### Why not ...?`
entries of one plain paragraph and a closing `See: ...` line naming a requirement or a
document that exists. An answer carries no history, provenance or paths into the code.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
FAQ = REPO_ROOT / "project_meta/faq.md"
REQUIREMENTS = REPO_ROOT / "project_meta/requirements"
ANSWER_MAX_WORDS = 60
NARRATIVE = (
    "the user",
    "previously",
    "originally",
    "formerly",
    "at first",
    "was changed",
    "used to",
    "no longer",
    "earlier proposal",
    "was proposed",
    "was rejected",
    "was decided",
)


def forbidden(text: str) -> list[str]:
    """What an answer may not hold, the same rules reqtool applies to requirement text."""
    findings = []
    for phrase in NARRATIVE:
        if re.search(rf"(?<![^\W_]){re.escape(phrase)}(?![^\W_])", text.lower()):
            findings.append(f'narrates history or provenance ("{phrase}")')
    if re.search(r"\d{4}-\d{2}-\d{2}", text):
        findings.append("holds a date")
    if re.search(r"(ADR-|#)\d", text):
        findings.append("names an ADR or a PR")
    if "::" in text or ".rs" in text or "src/" in text:
        findings.append("names a path into the code; modules declare their requirements")
    return findings


def unresolved(reference: str, base: Path, requirements: set[str]) -> str | None:
    link = re.fullmatch(r"\[.*?\]\((.*)\)", reference)
    if link:
        file = link.group(1).split("#")[0]
        if not (base / file).is_file():
            return f"links to {file}, which does not exist"
        return None
    if reference not in requirements:
        return f"{reference} is neither a requirement nor a Markdown link"
    return None


class Entry:
    def __init__(self, heading_line: int) -> None:
        self.heading_line = heading_line
        self.answer: list[str] = []
        self.paragraphs = 0
        self.in_paragraph = False
        self.see: tuple[int, str] | None = None

    def push(self, number: int, line: str, findings: list[str]) -> None:
        trimmed = line.strip()
        if not trimmed:
            self.in_paragraph = False
            return
        if self.see is not None:
            findings.append(f"line {number}: nothing follows an entry's `See:` line")
            return
        if trimmed.startswith("See: "):
            self.see = (number, trimmed[len("See: "):])
            return
        if trimmed[0] in "-*|>`" or trimmed.startswith("1."):
            findings.append(
                f"line {number}: an answer is one plain paragraph: no lists, tables, quotes or code"
            )
        if not self.in_paragraph:
            self.paragraphs += 1
            self.in_paragraph = True
        self.answer.append(trimmed)

    def finish(self, base: Path, requirements: set[str]) -> list[str]:
        findings = []
        at = self.heading_line
        answer = " ".join(self.answer)
        if self.paragraphs != 1:
            findings.append(f"entry at line {at}: the answer is one paragraph, got {self.paragraphs}")
        words = len(answer.split())
        if words > ANSWER_MAX_WORDS:
            findings.append(f"entry at line {at}: the answer is {words} words; at most {ANSWER_MAX_WORDS}")
        findings.extend(f"entry at line {at}: {finding}" for finding in forbidden(answer))
        if self.see is None:
            findings.append(f"entry at line {at}: ends with a `See:` line naming where the decision is kept")
            return findings
        number, see = self.see
        references = [reference.strip() for reference in see.split(",") if reference.strip()]
        if not references:
            findings.append(f"line {number}: `See:` names nothing")
        for reference in references:
            problem = unresolved(reference, base, requirements)
            if problem:
                findings.append(f"line {number}: {problem}")
        return findings


def check(text: str, base: Path, requirements: set[str]) -> list[str]:
    findings: list[str] = []
    entry: Entry | None = None
    in_topic = False
    for number, line in enumerate(text.splitlines(), start=1):
        if line.startswith("### "):
            if entry:
                findings.extend(entry.finish(base, requirements))
            heading = line[len("### "):]
            if not in_topic:
                findings.append(f"line {number}: an entry outside a `##` topic")
            if not (heading.startswith("Why not ") and heading.endswith("?")):
                findings.append(f'line {number}: an entry\'s heading is "Why not ...?", got "{heading}"')
            entry = Entry(number)
        elif line.startswith("## "):
            if entry:
                findings.extend(entry.finish(base, requirements))
            entry = None
            in_topic = True
        elif line.startswith("#") and number > 1:
            findings.append(f"line {number}: only `##` topics and `###` entries follow the title")
        elif entry:
            entry.push(number, line, findings)
    if entry:
        findings.extend(entry.finish(base, requirements))
    return findings


def requirement_ids(directory: Path) -> set[str]:
    ids: set[str] = set()
    for file in directory.rglob("*.yaml"):
        ids.update(re.findall(r"^\s*- id: (\S+)", file.read_text(), flags=re.MULTILINE))
    return ids


def main() -> int:
    findings = check(FAQ.read_text(), FAQ.parent, requirement_ids(REQUIREMENTS))
    for finding in findings:
        print(f"{FAQ.relative_to(REPO_ROOT)}: {finding}")
    if findings:
        print(f"{len(findings)} finding(s) in the FAQ")
        return 1
    print("the FAQ is well formed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
