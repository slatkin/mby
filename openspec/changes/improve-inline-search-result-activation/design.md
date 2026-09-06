## Context

See proposal.md and the Inline Search specification. Inline Search is owned by the active destination, while standard library focus, row actions, and track-selection mode remain destination-local presentation behavior.

## Goals / Non-Goals

**Goals:**
- Preserve ordinary result-row actions after a search-bar-to-row mouse gesture.
- Re-anchor an activated album result into its destination's standard library presentation and track-selection mode.

**Non-Goals:**
- Change query scoring, corpus loading, non-album result activation, router precedence, or responsive transfer.

## Decisions

### 1. Preserve semantic row action dispatch

The destination SHALL resolve the result row to the same stable target used by its ordinary list, then expose existing context-menu and shortcut semantics rather than forwarding mouse coordinates across boundaries. This keeps input interpretation and hit geometry with the painter/owner.

### 2. Re-anchor by stable album identity

Enter SHALL dismiss Inline Search and send the selected album's stable target to the owning destination's ordinary library state. The destination restores its natural grouping/pill position, focuses the target, and enters existing track-selection mode. Reusing target-based re-anchoring avoids preserving search indexes in normal library state.

## Risks / Trade-offs

- [A destination has no natural album position loaded] → use its existing load/navigation path before entering track-selection mode.
- [Mouse gesture state is handled outside the destination] → keep only semantic stable targets at boundaries; do not create a global mouse router.
