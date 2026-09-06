## Why

The current Wide hero sits directly beside the Queue panel, whose similar filled-panel treatment makes the left side of the app feel crowded relative to the right. Moving the hero to the far right places the denser Library list between those surfaces and gives the Wide presentation a more balanced visual hierarchy.

The placement-specific name `Hero-on-left` is also cumbersome. `Wide hero` distinguishes the presentation from `Inline hero` without encoding incidental geometry in its name.

## What Changes

- Rename the current `Hero-on-left` presentation to `Wide hero` throughout current domain language, architecture, source, and tests; archived OpenSpec history remains unchanged.
- Mirror the Wide hero arrangement so the single-column Library browser and its pills occupy the left pane while the selected-item hero or provider-owned detail workspace occupies the right pane.
- Preserve the existing gap, shared breakpoint, minimum-height guard, status-row reserve, focus treatment, content, list state, pointer behavior, and Inline hero fallback.
- Re-proportion the Wide hero split so the list/browser pane is the ~40% ratio-driven pane and the hero/workspace pane takes the remaining ~60% (previously the hero was the ~40% pane). The ratio driver moves to the browser width and is clamped so neither pane falls below the shared minimum at the breakpoint.
- Preserve existing focus and input behavior: Enter focuses structured media content and Esc returns to the Library list; this change introduces no keyboard-routing or directional-key changes.
- Add no unit tests. Evaluate affected existing tests individually, update tests that protect durable behavior, and remove or relax tests that only freeze obsolete placement or internal naming.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `right-panel-arrangements`: Rename the Wide presentation and reverse its pane placement while preserving shared arrangement behavior.
- `library-list-hero`: Place Wide hero content on the right and the single-column browser on the left.
- `canonical-media-lists`: Compose `WideMediaList` in the left browser pane of the Wide hero presentation.
- `library-list-columns`: Define the Wide hero browser as a single-column left pane.
- `music-library-hero`: Move grouped Music's album browser left and its album/track workspace right.
- `audiobookshelf-book-browsing`: Move the book browser left and persistent book/chapter workspace right.
- `audiobookshelf-podcast-browsing`: Move the show browser left and persistent show/episode workspace right.
- `audiobookshelf-podcast-library-ui`: Adopt the Wide hero name and mirrored pane placement.
- `feeds-service-wide-list`: Apply the established Wide hero list framing to the left browser pane.
- `queue-canonical-list`: Replace the retired presentation term in Queue's explicit exclusions without changing Queue behavior.
- `ui-design-language`: Replace the placement-specific presentation term while preserving semantic focus resolution.

## Impact

- Affects the shared hero arrangement and all Wide hero-bearing destinations: Home, Movies, TV shows, grouped Music, Emby podcasts, Emby home videos, Audiobookshelf podcasts, Audiobookshelf books, and Feeds.
- Affects current presentation terminology in `CONTEXT.md`, current ADR documentation, current OpenSpec specifications, render/component identifiers, comments, and placement-sensitive tests.
- Does not affect Remote Services, playback, Queue behavior, persisted state, protocols, dependencies, Normal/Mini presentation, or Inline hero behavior.
