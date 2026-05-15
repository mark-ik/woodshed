# Documentation Policy

Adapted from the graphshell DOC_POLICY for guitar-toolkit's narrower scope.
This project has a single subsystem (the app) plus a shared theory crate, so
the policy is intentionally light.

## Core Principles

### 1. Control Doc Growth

Add to existing docs unless the material is substantial (>500 words),
covers a distinct topic, and is unrelated to any current document. Keep
total doc count low. Do not create files for one-time analyses.

### 2. Eliminate Redundancy

Audit before commits or after substantial changes. Newer documents are
generally more authoritative. If two docs disagree, reconcile them — do
not let drift accumulate.

### 3. No Legacy Friction

When a path changes, optimize for clean fit with the new path. Do not
preserve obsolete parallel systems or migration shims unless explicitly
needed for real-user data. Tests track current semantics only.

### 4. Location and Archival

- **Active docs**: live directly in `design_docs/`. Subdirectories may be
  added when a domain (e.g. `theory_docs/`, `audio_docs/`) accumulates
  enough material to justify one. Until then, flat is fine.
- **Archive**: `design_docs/archive_docs/<YYYY-MM-DD>/` for retired plans
  and superseded notes. Move there rather than delete; delete only with
  rationale and confirmation.
- **Cross-references**: relative links.

### 5. README Requirements

`design_docs/DOC_README.md` is the canonical index for `design_docs/`. It
must contain:

- AI-assistant working principles for this project
- Index of all active docs with one-line descriptions
- Pointers to `DOC_POLICY.md` and `PROJECT_DESCRIPTION.md`

When docs are added, removed, or moved, `DOC_README.md` is updated in the
same session. If any other index disagrees with `DOC_README.md`,
`DOC_README.md` wins.

### 6. PROJECT_DESCRIPTION.md Ownership

`PROJECT_DESCRIPTION.md` is reserved for the maintainer. Do not edit
without explicit instruction. Treat it as authoritative; surface
contradictions for discussion.

The root `README.md` is derived from `PROJECT_DESCRIPTION.md` and current
authoritative docs. Speculative features without plans only appear in
`PROJECT_DESCRIPTION.md`.

### 7. Implementation Planning Documents

Plans for non-trivial code work go in `design_docs/` as
`<YYYY-MM-DD>_<keyword>_plan.md`. Each plan includes:

- **Plan**: phases and progress
- **Findings**: research and discoveries during execution
- **Progress**: session log and test results

Update the plan every two prompts or every two completed tasks. Re-read
the plan before resuming work on the same project. On completion, move
the plan to `archive_docs/<date>/`.

Organize plan tasks by **feature target and validation criteria**, not by
calendar time (no "Day 1 / Week 2" labels). State done conditions, not
estimates.

### 8. Workflow Rule for AI Assistants

Before starting a project, read `DOC_README.md` first, then this policy.
Any durable working principle gleaned from a session should be promoted
into `DOC_README.md`'s working-principles section in that same session.
