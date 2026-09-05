## MODIFIED Requirements

### Requirement: Inline replacement tracks the current selection independent of scroll position

The hero SHALL always reflect the selected item. In Wide hero, its screen position SHALL remain fixed while the browser cursor moves. In the inline presentation, its replacement position SHALL follow the active row in scrolling list flow, and scrolling SHALL keep the selected replacement addressable together. Wide read-only heroes SHALL derive selection solely from the left-hand browser. Interactive right workspaces SHALL continue deriving their parent item from the left-hand browser while their child cursor is active.

#### Scenario: Wide selection scrolls out of view
- **WHEN** the browser cursor moves to an item whose row is scrolled outside the visible left rail
- **THEN** the right hero or workspace updates to that item
- **AND** the right pane remains in the same position

#### Scenario: Child selection does not change the projected parent
- **WHEN** an episode, track, or chapter is selected in the right workspace
- **THEN** the right workspace continues showing the parent selected by the left-hand browser
- **AND** the left-hand browser cursor remains unchanged

#### Scenario: Inline selection is scrolled
- **WHEN** the active row crosses the visible inline browser area
- **THEN** scrolling keeps the active row and inline selected detail in navigable flow
- **AND** selected detail follows the active row rather than remaining pinned to a screen edge

#### Scenario: TV selection scrolled out of view
- **WHEN** the wide TV Series cursor moves outside visible left-rail rows
- **THEN** the right Series workspace updates and remains fixed in the right pane

#### Scenario: Selection scrolled out of view
- **WHEN** a wide read-only browser selection scrolls outside visible left-rail rows
- **THEN** the right hero continues projecting the selected item

#### Scenario: Episode selection does not change the projected Series
- **WHEN** an episode is selected in the left TV workspace
- **THEN** the workspace continues projecting the Series selected by the left-hand browser

#### Scenario: Narrow selection is scrolled
- **WHEN** the cursor crosses the visible inline browser area
- **THEN** scrolling keeps the active row and inline detail addressable together

### Requirement: Hero placement follows the responsive presentation

The selected item's hero or detail workspace SHALL be positioned by the shared right-panel presentation rather than a surface-specific renderer. The arrangement SHALL own pane placement, breakpoints, and rectangle splitting; the component SHALL own painting; and the screen SHALL provide semantic content and interaction state. When wide geometry is available, Wide hero SHALL place selected detail beside a single-column browser. Otherwise the selected ordinary browser row SHALL be replaced by the variable-height inline detail block in the single-column scrolling browser. No presentation SHALL reserve a separate full-width area above the browser.

The inline hero SHALL remain part of list flow as the selected row's replacement. Its variable height SHALL be budgeted once, its block SHALL own the selected item's geometry and parent activation target, and single click SHALL focus while double click performs normal item activation. If the replacement cannot fit, the ordinary selected row SHALL be restored with its normal selected appearance and interaction.

The inline hero SHALL render the same content shape on every surface: title, optional metadata line, optional overview text, and an optional image. The image model SHALL be selected by image aspect ratio — Model A (right-aligned, wrap-around) for tall images such as posters and book covers, Model B (right-half, meta-column) for wide 16:9 thumbnails. No surface SHALL render structured lists (seasons, episodes, tracks, chapters) inside the inline hero. Structured lists SHALL be accessed via the inline-hero selection modal (see `inline-hero-selection-modal`).

For wide Movies, the right hero SHALL continue using Home's selected-media card. For wide TV, the right workspace SHALL continue showing Series artwork, metadata, overview, season pills, and episodes. Other surfaces SHALL retain their declared content and interaction behavior while adopting the same placement rule. Wide-mode track and episode listings are outside this requirement; they are governed by the Wide hero presentation.

#### Scenario: Wide hero-bearing browse surface

- **WHEN** a hero-bearing browse surface meets the shared wide geometry conditions and has a selected item
- **THEN** selected detail renders in the right pane
- **AND** the single-column browser renders in the left rail

#### Scenario: Narrow library renders an inline hero

- **WHEN** a hero-bearing browse surface does not meet the shared wide geometry conditions and has a selected item
- **THEN** the selected item's ordinary row is replaced by its inline hero block at the same flow position
- **AND** the browser remains a single scrolling column
- **AND** the hero shows title, metadata, overview, and image using the model selected by the image's aspect ratio

#### Scenario: Narrow selection changes

- **WHEN** the cursor moves to another item in the inline presentation
- **THEN** the previous row returns to its ordinary presentation and the new selected row is replaced by inline detail
- **AND** the previous row returns to its ordinary presentation

#### Scenario: Selected inline hero reaches the viewport bottom

- **WHEN** the selected row's full inline hero would extend below the visible browser
- **THEN** the browser scrolls upward until the complete inline hero is visible
- **AND** every surface uses the shared inline-detail flow rather than surface-specific scrolling

#### Scenario: Narrow list has insufficient space

- **WHEN** the inline presentation cannot fit the minimum active row and minimum selected detail
- **THEN** the ordinary selected row is restored
- **AND** its normal selected appearance and interaction are retained

#### Scenario: Narrow TV shows uses standard hero with selection modal

- **WHEN** a TV Series is selected in the inline presentation
- **THEN** the inline hero shows the Series title, metadata, overview, and poster image only
- **AND** season pills and episode rows do NOT render inside the inline hero
- **AND** pressing Enter opens the constituent-list modal for season and episode selection

#### Scenario: Narrow grouped Music

- **WHEN** grouped Music uses the inline presentation
- **THEN** selected album hero content (title, metadata, album art) replaces the active album row
- **AND** the track list does NOT render inline; Enter opens the selection modal

#### Scenario: Narrow Audiobookshelf podcast

- **WHEN** an Audiobookshelf podcast library uses the inline presentation
- **THEN** selected-show hero content (title, author, description, cover) replaces the active show row
- **AND** filters and downloaded episodes do NOT render inside the inline hero
- **AND** alphabetical pills render in the panel area like every other library tab
- **AND** pressing Enter opens the constituent-list modal for episode selection

#### Scenario: Narrow Audiobookshelf book

- **WHEN** an Audiobookshelf book library uses the inline presentation
- **THEN** selected-book hero content (title, author, metadata, overview, cover) replaces the active book row
- **AND** chapter detail does NOT render inside the inline hero
- **AND** the cover image uses Model A (right-aligned, wrap-around), not Model B
- **AND** exactly one author-bucket pill row renders above the browser with a parent-background spacer
- **AND** no chapter child target or chapter focus exists in the narrow presentation
- **AND** Enter or parent double-click opens the chapter selection modal

#### Scenario: Narrow Feeds

- **WHEN** Feeds uses the inline presentation
- **THEN** selected-entry detail replaces the active entry row
- **AND** the hero shows title and metadata with no image (Model A degenerate)

#### Scenario: Narrow Home

- **WHEN** Home uses the inline presentation and its selected section has an item
- **THEN** selected-item detail replaces the active row in the selected section's list flow
- **AND** the section pills remain outside selected-item detail
- **AND** Home items with wide 16:9 artwork (Emby Keep Watching, Audiobookshelf episodes) use Model B (beside-image)
- **AND** Home Feed items use Model A no-image (text-only), matching the dedicated Feeds tab

#### Scenario: Wide TV pills sit in separate rails

- **WHEN** wide TV has both eligible library letter pills and season data for the selected Series
- **THEN** letter-range pills render at the top of the left-hand Series rail
- **AND** season pills render in the right Series workspace above its episode list

#### Scenario: Wide Movies pills sit in the right rail

- **WHEN** wide Movies is eligible for letter-range pills
- **THEN** the pill row renders at the top of the left-hand list rail
- **AND** the Movies list renders below it

#### Scenario: Inline selectors remain outside inert hero rows

- **WHEN** a surface has browser-level pills or search controls in the inline presentation
- **THEN** those controls retain their browser-level placement
- **AND** selected-item detail does not duplicate them

#### Scenario: Wide Movies renders the Home selected-media card

- **WHEN** a Movie is selected in wide Movies
- **THEN** the right pane renders the same selected-media card Home uses for that Movie

#### Scenario: Wide TV shows renders the selected Series workspace

- **WHEN** a Series is selected in wide TV
- **THEN** the right pane renders its artwork, metadata, season pills, and episodes
- **AND** the one-column Series browser remains in the left rail

#### Scenario: Wide TV season selection filters episodes

- **WHEN** the user selects another season in the wide TV workspace
- **THEN** only the right-pane episode list changes
- **AND** the left-hand Series browser remains unchanged

#### Scenario: Hero renders above the list

- **WHEN** a surface that formerly rendered selected detail above its list is displayed
- **THEN** it renders that detail on the right when wide or inline at the active row otherwise
- **AND** no separate top area is reserved

#### Scenario: Movies falls back below the breakpoint

- **WHEN** Movies does not meet the wide geometry conditions
- **THEN** selected Movie detail renders inline at the active row

#### Scenario: TV shows falls back below the breakpoint

- **WHEN** TV shows does not meet the wide geometry conditions
- **THEN** selected Series detail renders inline at the active row with title, metadata, overview, and poster only

#### Scenario: Narrow grouped Music uses selected-row replacement

- **WHEN** grouped Music uses the narrow presentation
- **THEN** selected album detail replaces the active album row with title, metadata, and album art only

#### Scenario: Wide grouped Music uses its side hero

- **WHEN** grouped Music meets the wide geometry conditions
- **THEN** selected album and tracks render in the right pane beside the one-column album browser

#### Scenario: Hero suppressed when too little space remains

- **WHEN** the active presentation cannot fit minimum selected detail and a usable active row
- **THEN** the ordinary selected row is restored and the browser uses the available area

#### Scenario: Letter pills sit between hero and list

- **WHEN** a surface uses browser-level letter pills
- **THEN** Wide hero places them in the left rail and inline presentation places them before browser flow
- **AND** they are never attached to a separate detail block

### Requirement: Selected replacement owns parent pointer behavior

A read-only Wide hero preview SHALL remain inert. In the inline presentation, the replacement block SHALL own the selected parent geometry: a single click focuses it and a double click performs normal item activation. Interactive child rows and selectors SHALL expose their existing navigation targets and take precedence over the parent target; no duplicate ordinary row or marker remains.

#### Scenario: Wide read-only hero remains inert
- **WHEN** a user clicks artwork or blank space in a read-only right hero
- **THEN** no media item is activated
- **AND** activation remains available from the left-hand browser row

#### Scenario: Wide interactive child row
- **WHEN** a user clicks an episode, track, or chapter row in an interactive right workspace
- **THEN** that child becomes selected according to the surface's existing interaction behavior

#### Scenario: Inline replacement parent target
- **WHEN** a user single-clicks inline hero framing or metadata that is not an explicit child or selector target
- **THEN** the selected parent item receives focus
- **AND** a double click performs normal parent activation

#### Scenario: Inline replacement child target
- **WHEN** a user clicks an explicit episode, track, chapter, or selector target inside inline detail
- **THEN** that child target handles the gesture without triggering parent activation

#### Scenario: Wide Movies hero remains read-only
- **WHEN** the wide Movies hero is displayed
- **THEN** it has no keyboard focus or pointer activation action

#### Scenario: Wide TV episode row click
- **WHEN** a user clicks a visible episode row in the wide TV right workspace
- **THEN** that episode becomes selected without changing the Series browser cursor

#### Scenario: Wide TV season pill click
- **WHEN** a user clicks a season pill in the wide TV right workspace
- **THEN** the season changes without playing an episode

#### Scenario: Wide TV artwork click
- **WHEN** a user clicks Series artwork or blank space in the wide TV right workspace
- **THEN** no episode is selected or played

#### Scenario: Single click on the replacement
- **WHEN** a user single-clicks non-interactive replacement framing or metadata
- **THEN** the selected parent is focused without activation

#### Scenario: Single click on the hero
- **WHEN** a user single-clicks non-interactive hero framing or metadata
- **THEN** the selected parent receives focus without activation

#### Scenario: Double click on the replacement
- **WHEN** a user double-clicks non-interactive replacement framing or metadata
- **THEN** the selected parent performs normal item activation

#### Scenario: Suppressed replacement restores the ordinary row
- **WHEN** selected detail is suppressed by available height
- **THEN** the ordinary selected row owns focus and normal activation

#### Scenario: Retired separate placement is not retained
- **WHEN** a formerly separate-detail surface adopts inline or Wide hero placement
- **THEN** selected-row and explicit child-target activation remain available
- **AND** no duplicate detail target remains

### Requirement: Hero content is independent of placement

Hero content SHALL be independent of responsive placement. The same surface declaration SHALL supply content to Wide hero and inline presentations, with only arrangement-specific composition changing. Wide Movies SHALL continue reusing Home's selected-media card rather than maintaining a second Movies-specific card. No hero content implementation SHALL depend on a separate placement fallback.

#### Scenario: Placement changes
- **WHEN** terminal geometry switches between Wide hero and inline presentation
- **THEN** selected detail preserves the content declared for that surface
- **AND** only placement and arrangement-specific composition change

#### Scenario: Home and wide Movies use one selected-media card
- **WHEN** the same Movie is selected in Home and in wide Movies
- **THEN** the hero card uses the same image selection, metadata, watch-state, overview treatment, and cache behavior

#### Scenario: Shared card changes centrally
- **WHEN** the shared Home selected-media card changes
- **THEN** wide Movies renders that change without a second Movies-card edit

#### Scenario: Hero content remains consistent
- **WHEN** selected detail switches between Wide hero and inline presentation
- **THEN** its declared image, metadata, overview, loading state, and child detail remain consistent

#### Scenario: Wide Movies card changes centrally
- **WHEN** the shared Home selected-media card presentation changes
- **THEN** wide Movies renders that change without a Movies-specific card edit
