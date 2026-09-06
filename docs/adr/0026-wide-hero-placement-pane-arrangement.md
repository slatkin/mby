---
status: accepted
supersedes: 0021-hero-placement-is-left-or-inline.md
---

# Wide Hero Placement And Pane Arrangement

Supersedes [ADR 0021](0021-hero-placement-is-left-or-inline.md). The accepted
history of ADR 0021 remains intact; this ADR only renames the responsive
presentation and fixes its pane order.

Hero-bearing browse surfaces still have exactly two supported presentations.
When the right panel meets the shared width breakpoint and the existing
minimum-height guard, the surface uses **Wide hero**. Otherwise it uses **Inline
hero**, which replaces the active media row in the single-column browser. These
two names form the complete hero-presentation vocabulary; neither name encodes a
physical side.

## Decision

The responsive hero presentation is named **Wide hero**, paired with **Inline
hero**. ADR 0021's `hero-on-left` term is retired from current source, tests,
live specs, `CONTEXT.md`, and current ADRs; archived OpenSpec history is not
rewritten.

Within Wide hero the shared arrangement places two panes named by semantic role,
not physical side:

- **browser** — the single-column Library browser and its browser-level pills,
  on the left pane.
- **hero** — the selected-item hero or provider-owned detail workspace, on the
  right pane.

Sizing is unchanged: the approximately 40%-width hero pane, the larger browser
pane, the existing gap, minimum pane widths, shared width breakpoint,
minimum-height guard, padding, and status-row reserve all carry over from ADR
0021. Only the pane order is mirrored. Destination renderers consume the
role-named geometry without re-splitting rectangles.

Pane focus remains browser focus versus structured-workspace focus, independent
of physical side. Enter/Esc transitions, key dispatch, typed requests, and
pointer targets do not change. See `openspec/changes/adopt-wide-hero/design.md`
D1 (rename) and D2 (mirror only the shared arrangement).

## Considered options

- **Retain `Hero-on-left` as an abstract name after moving the hero right:**
  rejected because it would make current architecture and code actively
  misleading.
- **Reverse panes independently in each destination:** rejected because it
  duplicates geometry and defeats the shared arrangement contract.
- **Remap keys to follow the new physical positions:** rejected because this
  change does not redefine focus navigation.

## Consequences

Placement is still recomputed from shared geometry whenever the panel changes
size or Panel mode changes. Wide output differs from the ADR 0021 layout only by
pane order; Normal and width-wide-but-short output are unchanged. Current
source, tests, live specs, `CONTEXT.md`, and current ADRs use only **Wide hero**
and **Inline hero**; archived OpenSpec history is not rewritten.
