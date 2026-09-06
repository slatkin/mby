## Why

Inline Search currently adds a three-row bordered input above library results, consuming scarce vertical space and changing the library panel's established structure. The search control should instead occupy the library's existing one-row pill slot so results retain the normal content origin and the panel remains visually stable.

## What Changes

- Replace the active library panel's pill controls with a one-row Inline Search bar in the pill slot.
- Preserve the pill slot's exact rectangle and background, including the existing parent-background spacer below it.
- Begin search results in the same content rectangle used by the normal library presentation.
- Ensure pill controls are neither painted nor mouse-active while Inline Search is active.
- Preserve existing query, loading, filtering, navigation, activation, dismissal, and Normal/Wide handoff behavior.
- Remove the three-row bordered Inline Search presentation rather than adding another layout or painter.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `inline-library-search`: Change the active Inline Search presentation from a three-row bordered input above the list to a one-row bar replacing the library pill row in place.

## Impact

- Affects the shared Inline Search arrangement and painter and the Browser, Music workspace, and TV workspace destination composition points.
- Updates rendering and component tests that encode the three-row input, pill visibility, hit eligibility, and result-area placement.
- Does not change service APIs, search data flow, dependencies, or ownership boundaries.
