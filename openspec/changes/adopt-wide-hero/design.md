## Context

The outer frame places Playback and Queue in the left column and Library content in the right panel. Within that Library panel, the shared `hero_on_left` arrangement currently places a roughly 40%-width hero/workspace on the left and the remaining browser rail on the right. This puts the similarly styled Queue and hero surfaces against the same central boundary.

The arrangement is shared, but destination Render Components and Interactive Components consume position-specific geometry and identifiers. Current specifications, `CONTEXT.md`, ADR 0021, comments, and tests also treat `Hero-on-left` as the canonical term. See `proposal.md` for motivation and the delta specs for required behavior.

## Goals / Non-Goals

**Goals:**
- Make the browser the left pane and the hero/workspace the right pane for every Wide hero destination.
- Replace the current placement-specific term with `Wide hero`, paired with `Inline hero`.
- Preserve one shared arrangement, one owner/painter per surface, and all existing responsive and interaction contracts.
- Keep the implementation no larger than the visual and terminology change requires.

**Non-Goals:**
- Changing the outer Playback/Queue/Library panel arrangement.
- Changing pane proportions, breakpoints, height guards, spacing, surfaces, or content.
- Changing keyboard routing, Enter/Esc focus behavior, directional-key behavior, mouse semantics, selection, scrolling, persistence, or effects.
- Adding unit tests or retaining tests that only encode obsolete placement or private names.

## Decisions

### D1: Rename the presentation to Wide hero

`Wide hero` names the responsive presentation without treating its current side as part of the domain concept. Together, `Wide hero` and `Inline hero` form the complete hero-presentation vocabulary.

Current source identifiers owned by this presentation SHALL use `wide_hero` naming. Temporary compatibility aliases are unnecessary because these identifiers are crate-private. Archived OpenSpec changes remain historical and SHALL NOT be rewritten. Current ADR 0021 SHALL be superseded by a new ADR recording Wide hero versus Inline hero; accepted history remains intact.

Alternative: retain `Hero-on-left` as an abstract name after moving it right. Rejected because it would make current architecture and code actively misleading.

### D2: Mirror only the shared Wide arrangement

The shared arrangement SHALL continue computing a 40%-width hero pane and a larger browser pane with the existing gap and minimum pane widths, but SHALL place the larger browser pane first and the hero pane second:

```text
Outer left column        Library panel
+-------------------+    +---------------------------+
| Playback          |    | browser       | hero      |
|-------------------|    | and pills     | workspace |
| Queue             |    |               |           |
+-------------------+    +---------------------------+
```

The returned geometry SHALL be named by semantic role (`browser`, `hero`) rather than physical side wherever practical. Destination-specific renderers SHALL consume those roles without re-splitting rectangles.

Alternative: reverse panes independently in each destination. Rejected because it duplicates geometry and defeats the shared arrangement contract.

### D3: Preserve semantic focus and input behavior

Pane focus remains browser focus versus structured-workspace focus, independent of physical side. Existing Enter/Esc transitions, key dispatch, typed requests, and pointer targets SHALL not change. Geometry used for painting and hit resolution SHALL move together through each owning component.

Directional-key behavior that already exists is explicitly outside this change. The implementation SHALL neither reinterpret nor clean it up as part of the visual move.

Alternative: remap keys to follow new physical positions. Rejected because this change does not redefine focus navigation and the requested contract is Enter to focus media content and Esc to return.

### D4: Reuse existing verification and remove brittle placement assertions

No new unit test SHALL be added. Existing tests affected by renamed identifiers or mirrored coordinates SHALL be evaluated individually:

- Keep and update tests that protect shared breakpoint selection, status-row reserve, one-painter ownership, focus semantics, hit geometry, or state preservation.
- Remove or relax tests whose only value is asserting that the hero is specifically left, the browser is specifically right, or a crate-private helper has its old name.
- Use existing rendered coverage plus live inspection at representative Wide, Normal, and Wide-but-short geometry. Normal and short output should remain unchanged; Wide output should differ only by pane order and terminology is not visible.

Alternative: add a new mirror-specific unit-test matrix. Rejected because existing rendering coverage and direct visual inspection already exercise these surfaces, while another coordinate snapshot would duplicate implementation details.

## Risks / Trade-offs

- [A destination consumes physical `left`/`right` fields rather than semantic roles] -> Rename shared geometry first, then let compiler errors expose callers; inspect every Wide hero destination before changing it.
- [Painted and pointer geometry diverge] -> Move each component's published/hit geometry with the pane it paints and verify existing pointer tests plus live interaction.
- [A destination silently keeps the old arrangement] -> Search current source and specifications for `hero_on_left`, `Hero-on-left`, and position-specific rail/workspace language; archived changes are the only allowed historical matches.
- [Broad terminology churn obscures the visual diff] -> Keep the source rename mechanical and separate from the minimal geometry change where practical; do not refactor unrelated rendering.
- [Removing brittle tests discards useful protection] -> Require each affected test to state the durable failure it catches before retaining, updating, or deleting it.

## Migration Plan

1. Update current domain/spec/ADR vocabulary to establish Wide hero and its pane roles.
2. Rename the shared arrangement and role-based geometry, then mirror its pane placement without changing sizing or responsive policy.
3. Update each destination to consume browser/hero geometry and move its published pointer regions with its painting.
4. Evaluate affected existing tests, deleting obsolete placement-only assertions and updating durable behavioral checks; add no unit tests.
5. Run formatting, targeted existing tests, architecture and size gates, then inspect all affected destinations at Wide, Normal, and Wide-but-short geometry.

Rollback is a single change reversal because there is no persisted-data, protocol, or dependency migration.
