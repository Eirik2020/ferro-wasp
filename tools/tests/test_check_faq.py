"""The FAQ's format, as tools/check_faq.py reads it."""

import unittest
from pathlib import Path

from tools import check_faq

NOWHERE = Path("/nonexistent")
IDS = {"X-001"}
GOOD = """# FAQ

Preamble.

## Links

### Why not one shared channel?

One requester could fill it for every other, and a
sender would have to name itself.

See: X-001
"""


class FaqTests(unittest.TestCase):
    def test_a_well_formed_faq_has_no_findings(self) -> None:
        self.assertEqual(check_faq.check(GOOD, NOWHERE, IDS), [])

    def test_an_entry_keeps_its_shape(self) -> None:
        bad = (
            "# FAQ\n\n### Why not early?\n\nOrphan.\n\nSee: X-001\n\n## Links\n\n"
            "### Should we use a channel\n\n- a list\n\nAnd a second paragraph.\n\n"
            "### Why not later?\n\nNo reference here.\n"
        )
        findings = "\n".join(check_faq.check(bad, NOWHERE, IDS))
        for needle in (
            "outside a `##` topic",
            '"Why not ...?"',
            "no lists",
            "one paragraph, got 2",
            "ends with a `See:` line",
        ):
            self.assertIn(needle, findings)

    def test_a_reference_must_resolve(self) -> None:
        text = (
            "# FAQ\n\n## Links\n\n### Why not a mailbox?\n\nIt keeps only the newest.\n\n"
            "See: X-002, [missing](nowhere.md)\n"
        )
        findings = "\n".join(check_faq.check(text, NOWHERE, IDS))
        self.assertIn("X-002 is neither", findings)
        self.assertIn("nowhere.md", findings)

    def test_an_answer_holds_no_history_or_paths(self) -> None:
        text = (
            "# FAQ\n\n## Links\n\n### Why not a mailbox?\n\nIt was rejected in #58 "
            "on 2026-01-02, and was changed in src/a.rs.\n\nSee: X-001\n"
        )
        findings = "\n".join(check_faq.check(text, NOWHERE, IDS))
        for needle in ("was changed", "names an ADR or a PR", "holds a date", "path into the code"):
            self.assertIn(needle, findings)


if __name__ == "__main__":
    unittest.main()
