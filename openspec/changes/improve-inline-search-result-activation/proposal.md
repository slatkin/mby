## Why

Inline Search can find an album, but result interaction does not retain the standard library actions or give Enter a useful way to return to that album's normal context. Selecting a result should preserve its row actions and make the album ready for track selection in the ordinary library presentation.

## What Changes

- Keep result-row context-menu actions and Ctrl+P, Ctrl+S, and Ctrl+A available after mouse-down moves from the Inline Search bar to a result row.
- Make Enter on a selected album result dismiss Inline Search, restore the standard library presentation, focus that album in its natural pill/list position, and enable track-selection mode.
- Preserve existing search filtering, navigation, and responsive transfer behavior for results that are not activated this way.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `inline-library-search`: Define result-row action availability and album-result activation back into the standard library presentation.

## Impact

Affects Inline Search input/result dispatch, destination re-anchoring into the ordinary library presentation, and focused interaction tests. No Service API, persistence format, or dependency changes.
