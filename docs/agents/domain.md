# Domain Docs

Read `GLOSSARY.md` at the repo root (and any relevant ADR in `docs/adr/` if present) before exploring or naming concepts. If `docs/adr/` does not exist yet, proceed silently.

## Vocabulary

Use the exact terms defined in `GLOSSARY.md` (notch, closed notch, open notch, dwell, page, ambient cover, synced lyrics, fake player, demo spec) in code, tests, issues, and commit messages. Avoid synonyms the glossary rejects (such as "bar", "panel", "dock", "island" for the shell window itself, or "tab" for a page).

## ADR conflicts

When a change contradicts an existing ADR in `docs/adr/`, call out the conflict explicitly rather than overriding it silently.
