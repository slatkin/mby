## MODIFIED Requirements

### Requirement: WideMediaList owns fixed-row mechanics
`WideMediaList<Target>` SHALL be a persistent embedded plain TuiRealm `Component` that owns cursor, scroll, viewport, fixed-height one-column row placement, semantic painting delegation, scrollbar, movement, clamping, and internal row geometry for painting and scrolling. It SHALL support Wide hero rails and later Queue fixed rows, but SHALL NOT implement Inline replacement or a non-hero two-column policy. It SHALL express letter grouping through `MediaListRow::Heading`/`Spacer` rows. An applicable Wide Browser path SHALL delegate to this control and SHALL NOT reach `render_generic_movies_home_video_rows_with_ctx` or either painter it routes to (`render_letter_grouped_rows`, `render_plain_rows`); the absence of a `render_plain_rows` call alone SHALL NOT be accepted as compliance. It SHALL expose no mouse hit-resolution API; `restore-mouse-support` (#638) adds `HitRegions<Target>` later.

#### Scenario: Wide TV rail composes the control
- **WHEN** the TV surface is Wide hero
- **THEN** its left rail is painted and interacted with by one `WideMediaList`
- **AND** the parent retains workspace, hero, images, and effects

### Requirement: Named destinations compose without changing provider authority
The slice SHALL compose persistent `WideMediaList` controls in the applicable Wide hero paths and persistent `InlineMediaBrowser` controls in the applicable Narrow paths for hero-bearing generic Emby catalogs, Movies, the Emby homevideos feed view, the Emby podcast channel list, and TV Series browsing. Non-hero two-column Emby catalogs SHALL keep their existing two-column arrangement policy and SHALL NOT be forced onto either canonical control. Provider workspaces, images, effects, persistence, Service and Player authority, and typed message translation SHALL remain in their existing parents/shell.

#### Scenario: One painter is active
- **WHEN** a listed destination is rendered at its applicable breakpoint
- **THEN** exactly one list painter runs
- **AND** the old loop is not run as an underpaint

### Requirement: Provider destinations compose canonical media controls

Grouped Music album browsing and Audiobookshelf Podcast show browsing and Book browsing SHALL prepare provider-owned content as canonical selectable `Item`, non-selectable `Heading`, and `Spacer` rows and compose `WideMediaList` for Wide rails and `InlineMediaBrowser` for Normal/Narrow selected-row replacement where the arrangement permits. The controls SHALL remain embedded beneath the mounted destination component; provider workspaces, images, selectors, surname buckets, effects, and typed intent translation remain parent-owned.

#### Scenario: Music groups retain provider authority
- **WHEN** a grouped Music album surface is rendered or navigated
- **THEN** album/group rows use the canonical control
- **AND** grouping, track authority, images, selection, and playback intents remain Music-owned
- **AND** no second list painter or App-owned interaction mirror runs.

#### Scenario: Audiobookshelf shows compose without losing episodes
- **WHEN** a Podcast library is shown Wide or Normal
- **THEN** shows use the canonical list presentation
- **AND** the selected show's episode workspace remains provider-owned, including episode filtering, images, and typed playback intents.

#### Scenario: Audiobookshelf books compose without duplicate detail
- **WHEN** a Book library is shown Wide
- **THEN** the selected book's persistent provider detail workspace renders in the right pane and the left rail shows ordinary fixed-height one-column canonical rows
- **AND** no selected-row replacement and no Inline hero is painted in the Wide left rail
- **AND** chapter rows remain provider-owned seek targets for the selected book.

### Requirement: Audiobookshelf geometry has complete breakpoint fallbacks

Audiobookshelf Podcast and Book surfaces SHALL use the shared Wide hero or Inline arrangement at the established Wide/Normal breakpoints, preserve the short-height fallback, and hand off stable selected target and viewport anchor across breakpoint changes. Non-list repairs required to make the composition correct SHALL live in shared arrangements or the owning destination component, not a bespoke exception.

#### Scenario: Wide and short layouts are deterministic
- **WHEN** terminal width/height crosses the Wide threshold or the short-height guard
- **THEN** the surface selects the defined Wide, Normal, or short fallback arrangement
- **AND** the selected target, row offset, images, framing, and focus remain stable.

### Requirement: TV and Movies establish the Wide composition precedent

When grouped Music or an Audiobookshelf Podcast or Book destination meets the shared Wide width and minimum-height predicate, it SHALL follow the TV/Movies composition: its provider-owned detail/workspace SHALL occupy the right pane, and its parent-owned browser-level pills, followed by ordinary one-column canonical rows, SHALL occupy the left rail. The arrangement SHALL use the same shared predicate, pane framing, content spacing, and short-height fallback as TV/Movies. The Wide presentation SHALL NOT use an Inline hero or selected-row replacement in the left rail; when the shared predicate is not met, the destination SHALL use the shared Inline fallback (or suppress detail when the shared minimum cannot fit), not a bespoke arrangement. The arrangement mechanics of this precedent — shared predicate, pane framing, content spacing, and short-height fallback — are specified by the `right-panel-arrangements` spec; this requirement governs only how the canonical controls compose into that arrangement.

#### Scenario: Wide provider workspace and ordinary rail
- **WHEN** grouped Music or an Audiobookshelf Podcast or Book destination meets the shared Wide geometry conditions
- **THEN** its provider-owned detail/workspace is on the right
- **AND** its browser-level pills and ordinary one-column canonical rows are on the left
- **AND** no Wide Inline hero or selected-row replacement is painted in the left rail.

#### Scenario: Shared predicate and fallback apply
- **WHEN** the destination crosses the shared width or minimum-height guard
- **THEN** it uses the same predicate, pane framing, content spacing, and short-height fallback as TV/Movies
- **AND** it does not introduce a destination-specific arrangement or breakpoint.
