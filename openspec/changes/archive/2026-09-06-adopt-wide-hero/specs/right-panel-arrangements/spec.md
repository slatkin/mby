## MODIFIED Requirements

### Requirement: Each screen is assigned one wide arrangement

Every hero-bearing right-panel browse surface SHALL use Wide hero for its wide presentation. This includes Home, Movies, TV shows, grouped Music, Emby podcasts, Emby home videos, Audiobookshelf podcasts, Audiobookshelf books, and Feeds. A read-only selected-item hero SHALL remain a projection of the left-hand browser selection. A surface whose right detail workspace contains episodes, tracks, or chapters MAY expose that existing interactive content without changing the shared placement rule. No hero-bearing browse surface SHALL declare a separate detail placement or a surface-specific responsive placement.

#### Scenario: Wide read-only hero surface
- **WHEN** Home, Movies, an Emby home-video library, or Feeds is displayed with wide geometry
- **THEN** the selected-item hero renders in the right pane
- **AND** the left rail remains the only focusable browser pane

#### Scenario: Wide interactive detail surface
- **WHEN** TV shows, grouped Music, an Audiobookshelf podcast library, or an Audiobookshelf book library is displayed with wide geometry
- **THEN** the selected item's persistent detail workspace renders in the right pane
- **AND** the single-column catalog browser renders in the left rail
- **AND** existing episode, track, or chapter focus behavior remains available where that surface already provides it

#### Scenario: Movies is displayed at a wide width
- **WHEN** the dedicated Movies library meets the wide geometry conditions
- **THEN** the selected-media hero is on the right
- **AND** the letter-range pills and one-column Movies list are in the left rail

#### Scenario: TV shows is displayed at a wide width
- **WHEN** the TV shows library meets the wide geometry conditions
- **THEN** the selected Series detail, season pills, and persistent episode preview are on the right
- **AND** TV letter-range pills and the one-column Series list are in the left rail

#### Scenario: Feeds is displayed at a wide width
- **WHEN** Feeds meets the wide geometry conditions
- **THEN** the selected entry's hero is on the right
- **AND** group and watched selectors plus the one-column entry browser are in the left rail

#### Scenario: Audiobookshelf podcast library is displayed at a wide width
- **WHEN** an Audiobookshelf podcast library meets the wide geometry conditions
- **THEN** the selected show and its filtered episode workspace are on the right
- **AND** the one-column podcast-show browser is in the left rail

#### Scenario: Audiobookshelf book library is displayed at a wide width
- **WHEN** an Audiobookshelf book library meets the wide geometry conditions
- **THEN** it renders the Wide hero arrangement matching grouped Music at the same dimensions

#### Scenario: Hero-bearing surface leaves wide geometry
- **WHEN** any hero-bearing browse surface no longer meets the shared wide geometry conditions
- **THEN** it renders its selected detail inline in a single-column browser
- **AND** no separate fallback is used

#### Scenario: Wide TV shows has an interactive left hero
- **WHEN** TV shows meets the wide geometry conditions
- **THEN** Series browsing remains on the left and the interactive episode workspace remains on the right

#### Scenario: Wide Movies has its selected-media hero
- **WHEN** Movies meets the wide geometry conditions
- **THEN** its selected-media hero renders on the right and its one-column browser on the left

#### Scenario: TV shows falls below the breakpoint
- **WHEN** TV shows does not meet the wide geometry conditions
- **THEN** selected Series detail replaces its ordinary row in its one-column browser

#### Scenario: Movies falls below the shared breakpoint
- **WHEN** Movies does not meet the wide geometry conditions
- **THEN** selected Movie detail replaces its ordinary row in its one-column browser

#### Scenario: Home videos is displayed at a wide width
- **WHEN** an Emby home-video library meets the wide geometry conditions
- **THEN** it renders Wide hero with a one-column left-rail browser

#### Scenario: Audiobooks is displayed at a wide width
- **WHEN** an Audiobookshelf book library meets the wide geometry conditions
- **THEN** it renders Wide hero matching grouped Music

### Requirement: The Wide hero right pane is a shared filled container

Every Wide hero destination's right pane SHALL be painted by a single shared arrangement
primitive. That primitive SHALL derive the right pane's extent from the shared Wide hero
presentation itself, fill that extent, and return the one shared content-inset rect that
destinations lay their hero content into. A destination SHALL NOT be able to supply a right-pane
extent of its own to the primitive.

The fill SHALL be unconditional: it does not depend on whether an item is selected, whether the
destination has hero data, which provider supplied the item, or how tall the painted content is.
A destination SHALL NOT resize, re-derive, clamp, or conditionally skip the fill, and SHALL NOT
apply a destination-specific content inset. The right pane's content inset SHALL be the single
shared pane inset used by every Wide hero destination; no destination defines its own.

The status-row reserve remains owned solely by the shared Wide hero presentation: the filled
right pane SHALL bottom out exactly one row above the status bar on every destination, in every
selection state, at every Wide geometry. Destinations SHALL NOT paint a separate strip below the
right pane to simulate that reserve.

#### Scenario: Every Wide hero destination fills its right pane

- **WHEN** any Wide hero destination renders at Wide geometry
- **THEN** every cell of its right pane carries the shared hero-pane surface
- **AND** no cell of the right pane shows the left column's backdrop surface

#### Scenario: Nothing is selected

- **WHEN** a Wide hero destination renders at Wide geometry with no selected item, no hero
  data, or an empty library
- **THEN** its right pane is still filled to its full extent
- **AND** only the pane's content is absent, not the pane

#### Scenario: Hero content is shorter than the pane

- **WHEN** a Wide hero destination's hero content occupies fewer rows than the right pane
- **THEN** the content is anchored to the top of the pane's content inset
- **AND** the pane's painted extent is unchanged by the content's height

#### Scenario: The right pane bottoms out one row above the status bar

- **WHEN** any Wide hero destination renders at Wide geometry
- **THEN** the filled right pane's bottom edge is exactly one row above the status bar
- **AND** no destination paints an additional row of any surface below the right pane

#### Scenario: A destination attempts to supply its own pane extent

- **WHEN** a destination has computed or mutated a right-pane rect of its own
- **THEN** that rect cannot be used to paint the pane
- **AND** the painted extent is the one the shared Wide hero presentation produced

#### Scenario: Every destination uses one content inset

- **WHEN** two Wide hero destinations render at the same Wide geometry
- **THEN** their hero content begins at the same offset from their right pane's edges

### Requirement: Pane focus treatment follows one rule

A Wide hero right pane SHALL render the focused surface treatment when, and only when, that
pane hosts a workspace that can hold focus and that workspace currently holds focus. This rule
SHALL be resolved by the shared pane primitive, not by each destination: a destination SHALL
declare only which of the two closed kinds its right pane is — a read-only hero, or a focusable
workspace together with that workspace's current focus state — and the primitive SHALL derive
the surface treatment from that declaration. A destination SHALL NOT be able to declare a
read-only pane as focused, and SHALL NOT select a surface treatment directly.

A right pane whose content is a read-only projection of the left rail's selection SHALL always
render the resting surface treatment, regardless of whether the right panel or the left rail is
focused.

#### Scenario: A focusable left workspace holds focus

- **WHEN** a Wide hero destination whose right pane hosts a focusable workspace has focus in
  that workspace
- **THEN** its right pane renders the focused surface treatment

#### Scenario: A focusable left workspace does not hold focus

- **WHEN** the same destination's focus is in the left rail, or the right panel is unfocused
- **THEN** its right pane renders the resting surface treatment

#### Scenario: A read-only hero pane never renders as focused

- **WHEN** a destination whose right pane is a read-only hero renders in any focus state
- **THEN** its right pane renders the resting surface treatment

### Requirement: Wide hero presents up to two focusable panes

The Wide hero arrangement SHALL present up to two panes, of which at most one is focused, and
only while the right panel itself is focused. A screen with a read-only hero pane — Home, the
wide Movies library, and Feeds — SHALL expose only its left-hand list as focusable content. A
screen whose right pane hosts an interactive workspace — the wide TV shows library, grouped
Music, an Audiobookshelf book library, and an Audiobookshelf podcast library — SHALL expose both
the left-hand list and that right workspace as focusable content. While left-rail browsing is
active, the right pane SHALL remain a projection of the selected item; when the right workspace's
selection is active, the right pane SHALL receive focus.

#### Scenario: Wide Movies has Library focus

- **WHEN** the wide Movies library is displayed and the Library panel has focus
- **THEN** the left-hand Movies list is the focused pane
- **AND** the right selected-media hero remains read-only and does not become a second focus target

#### Scenario: Wide TV shows has Series-list focus

- **WHEN** the wide TV shows library is displayed and episode selection is inactive
- **THEN** the left-hand Series list is the focused pane
- **AND** the right Series and episode workspace renders as an unfocused preview

#### Scenario: Wide TV shows has episode focus

- **WHEN** episode selection is active in the wide TV shows library
- **THEN** the right-hand episode workspace is the focused pane
- **AND** the left-hand Series list renders its unfocused treatment

#### Scenario: An Audiobookshelf podcast workspace takes focus

- **WHEN** episode selection is active in a wide Audiobookshelf podcast library
- **THEN** the right-hand episode workspace is the focused pane and renders the focused surface
  treatment
- **AND** the left-hand show list renders its unfocused treatment

#### Scenario: Focus moves between panes

- **WHEN** the user moves focus within a Wide hero screen that has focusable hero content
- **THEN** exactly one pane is focused and the other renders its unfocused appearance

#### Scenario: The right panel is unfocused

- **WHEN** the right panel is not focused
- **THEN** neither pane of a Wide hero screen renders as focused

### Requirement: Hero overview and media-list boxes have distinct ownership

Every Wide hero destination SHALL paint a recessed overview main-content box through the
shared primitive, even when its description is empty. The overview box carries only the Hero
text description and has one primitive-owned internal padding value.

A destination with structured episode, track, or chapter content SHALL additionally paint a
separate recessed media-list box. The shared arrangement owns both box and viewport rects; the
destination component owns its embedded `WideMediaList<Target>`, including rows, target identity,
cursor, scroll, selection, intent translation, and hit geometry. A Hero SHALL NOT carry a
structured listing or mutable list state. The destination SHALL NOT define its own box geometry
or surface.

#### Scenario: TV presents overview before episodes

- **WHEN** a selected Series renders at Wide geometry
- **THEN** title and ordered metadata render first
- **AND** one blank row separates the metadata from the overview main-content box
- **AND** a separate media-list box follows the overview box
- **AND** season pills are parent chrome above the episode `WideMediaList` viewport

#### Scenario: A structured workspace renders its media list

- **WHEN** TV, Music, or Audiobookshelf renders selected structured content
- **THEN** its parent-owned `WideMediaList` renders inside the separate media-list box
- **AND** canonical row, scroll, selection, and hit geometry are preserved

#### Scenario: The overview is empty

- **WHEN** a selected item supplies no description text
- **THEN** the overview main-content box is still painted
- **AND** its absence is never used to signal an empty payload

#### Scenario: Two overview payloads are compared

- **WHEN** two description payloads render in the overview main-content box at the same pane width
- **THEN** both begin at the same offset from the box's edges

### Requirement: The right panel has exactly two hero presentations

The right panel SHALL provide exactly two responsive hero presentations for every hero-bearing browse surface. At or above the shared breakpoint, when the existing minimum-height guard is satisfied, the surface SHALL use Wide hero: the selected hero or detail workspace occupies the right pane and a single-column browser occupies the left rail. Otherwise the surface SHALL use selected-row replacement: the selected item's ordinary row is replaced by its variable-height detail block in the single-column scrolling browser.

A separate detail block SHALL NOT be an arrangement or fallback. A surface SHALL NOT reserve a hero in a separate full-width area above its browser. Non-hero screens retain their existing presentation.

The inline hero SHALL render one content shape across all surfaces: title, optional metadata line, optional overview text, and an optional image. The image model SHALL be selected by image aspect ratio — right-aligned wrap-around (Model A) for tall images such as posters and book covers, right-half meta-column (Model B) for wide 16:9 thumbnails, and Model A's degenerate no-image form for surfaces without artwork. No surface SHALL render structured lists (seasons, episodes, tracks, chapters) inside the inline hero; those SHALL be accessed via the inline-hero selection modal.

#### Scenario: A browse surface enters the narrow presentation

- **WHEN** a hero-bearing browse surface's available width falls below the shared breakpoint
- **THEN** it renders one browser column
- **AND** the selected item's ordinary row is replaced by inline detail at the same flow position
- **AND** the inline hero shows title, metadata, overview, and image using the model selected by the image's aspect ratio
- **AND** no separate hero area is reserved above the browser
- **AND** no structured lists render inside the inline hero

#### Scenario: Wide geometry has insufficient height

- **WHEN** a hero-bearing browse surface meets the shared width breakpoint but fails the existing minimum-height guard
- **THEN** it uses selected-row replacement
- **AND** it restores the ordinary selected row if detail cannot fit

#### Scenario: A browse surface enters the wide presentation

- **WHEN** a hero-bearing browse surface meets the shared width and minimum-height conditions
- **THEN** it renders Wide hero
- **AND** its browser is a single-column left rail

#### Scenario: Panel mode changes

- **WHEN** the user cycles Panel mode
- **THEN** the presentation is recomputed from the width and height available to the right panel
- **AND** the same shared breakpoint and minimum-height guard apply

#### Scenario: A library enters the narrow presentation

- **WHEN** a library browse surface does not meet the shared wide geometry conditions
- **THEN** it renders one list column with selected detail inline at the active row
- **AND** the inline hero shows one content shape (title, metadata, overview, image) with no structured lists

#### Scenario: A formerly separate-detail surface crosses the breakpoint

- **WHEN** a formerly separate-detail surface crosses below the shared breakpoint
- **THEN** it uses selected-row replacement and retains no separate detail assignment

#### Scenario: A formerly separate-detail surface crosses the breakpoint

- **WHEN** a formerly separate-detail surface crosses the shared breakpoint in either direction
- **THEN** it switches only between Wide hero and selected-row replacement

#### Scenario: A wide hero screen falls below the breakpoint

- **WHEN** a Wide hero surface crosses below the shared breakpoint
- **THEN** it renders selected-row replacement with one browser column

#### Scenario: A Wide hero screen falls below the breakpoint

- **WHEN** a Wide hero surface no longer meets either wide geometry condition
- **THEN** it renders selected-row replacement

### Requirement: Shared Wide hero arrangement owns the status-row reserve
The shared Wide hero arrangement primitive SHALL reserve the one status-bar row when it computes the hero and list panes, so every Wide hero destination inherits the reserve from one place. Screens and components SHALL NOT re-derive the reserve (no per-tab `saturating_sub(1)`, `bottom_pad`, or equivalent) on top of the panes the shared primitive returns.

#### Scenario: Panels leave one blank row above the status bar
- **WHEN** any Wide hero destination (Home, Feeds, and the non-migrated media tabs that share the primitive) renders in the Wide layout
- **THEN** exactly one blank row separates the bottom of the content panels from the status bar, and that reserve is applied by the shared arrangement primitive rather than the screen.

### Requirement: Music and Audiobookshelf adopt the TV and Movies Wide precedent

Grouped Music and Audiobookshelf Podcast and Book destinations SHALL use the same Wide right-panel contract established by TV and Movies. When the shared width and minimum-height predicate is satisfied, the provider-owned detail/workspace SHALL occupy the right pane, while parent-owned browser-level pills followed by ordinary one-column rows SHALL occupy the left rail. The arrangement SHALL reuse the shared predicate, pane framing, content spacing, and short-height fallback. No Wide presentation SHALL use an Inline hero or selected-row replacement in the left rail.

#### Scenario: Wide provider workspace and ordinary right rail
- **WHEN** grouped Music or an Audiobookshelf Podcast or Book destination meets the shared Wide geometry conditions
- **THEN** its provider-owned detail/workspace renders in the right pane
- **AND** its browser-level pills and ordinary one-column rows render in the left rail
- **AND** the left rail contains no Wide Inline hero or selected-row replacement.

#### Scenario: Shared geometry fallback is retained
- **WHEN** the destination crosses the shared width or minimum-height guard
- **THEN** it uses the same shared predicate, pane framing, content spacing, and short-height fallback as TV and Movies
- **AND** it uses the shared Inline fallback, or suppresses detail when the shared minimum cannot fit
- **AND** it does not define a destination-specific breakpoint or arrangement.

### Requirement: Audiobookshelf Podcast and Book Wide surfaces route through the shared right pane

The Audiobookshelf Podcast Wide surface SHALL render through the shared Wide hero left-pane arrangement rather than a bespoke painter. The Audiobookshelf Book Wide left rail already routes through the shared left pane; its defect is that the `render_book_browser` call reused there carries the inline selected-row replacement path, and that replacement path SHALL NOT be used in the Wide left rail. These are provider-arrangement repairs this slice owns, distinct from the canonical list control itself. The Podcast Wide left rail SHALL present the same pill row it presents at Narrow width. The Book Wide right pane SHALL use the shared provider-detail-workspace framing and content spacing used by grouped Music, and its left rail SHALL show ordinary fixed-height one-column rows with no selected-row replacement and no Inline hero. Neither surface SHALL define a destination-specific breakpoint, column-sizing rule, or fallback.

#### Scenario: Podcast Wide has pill-row parity with Narrow
- **WHEN** an Audiobookshelf Podcast library meets the shared Wide geometry conditions
- **THEN** its left rail renders the shared pill row over the one-column show browser
- **AND** it routes through the shared Wide hero left pane, not a surface-specific painter.

#### Scenario: Book Wide uses shared workspace framing
- **WHEN** an Audiobookshelf Book library meets the shared Wide geometry conditions
- **THEN** the selected book's provider detail workspace renders in the right pane with the shared framing and spacing used by grouped Music
- **AND** the left rail renders ordinary fixed-height one-column rows with no selected-row replacement or Inline hero.

## RENAMED Requirements

- FROM: `The hero-on-left left pane is a shared filled container`
- TO: `The Wide hero right pane is a shared filled container`

- FROM: `Hero-on-left presents up to two focusable panes`
- TO: `Wide hero presents up to two focusable panes`

- FROM: `Left-pane focus treatment follows one rule`
- TO: `Pane focus treatment follows one rule`

- FROM: `Shared hero-on-left arrangement owns the status-row reserve`
- TO: `Shared Wide hero arrangement owns the status-row reserve`

- FROM: `Audiobookshelf Podcast and Book Wide surfaces route through the shared right pane`
- TO: `Audiobookshelf Podcast and Book Wide surfaces route through the shared left pane`
