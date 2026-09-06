## MODIFIED Requirements

### Requirement: Grouped Music uses responsive compositions

The grouped Music album view SHALL use Wide hero when it meets the shared wide geometry conditions. Its right pane SHALL contain album detail and tracks, and its left rail SHALL contain a single-column album browser. Otherwise the selected album detail SHALL replace the active album row in a single-column browser. The inline hero SHALL show album title, metadata, and album art only — no track list. The track list SHALL be accessed via the inline-hero selection modal (see `inline-hero-selection-modal`). Grouped Music SHALL NOT evaluate the breakpoint or minimum-height guard itself and SHALL NOT use a separate fallback.

#### Scenario: Grouped Music below the breakpoint

- **WHEN** grouped Music does not meet the shared wide geometry conditions
- **THEN** group pills span the content width
- **AND** albums render one per row
- **AND** the selected album's hero (title, metadata, album art) replaces its active row
- **AND** the track list does NOT render inline; Enter opens the selection modal

#### Scenario: Grouped Music at the breakpoint

- **WHEN** grouped Music meets the shared wide geometry conditions
- **THEN** it renders Wide hero with album detail and tracks in the right pane

#### Scenario: Grouped Music lacks sufficient height

- **WHEN** grouped Music meets the width breakpoint but fails the existing minimum-height guard
- **THEN** it renders the inline presentation with title, metadata, and album art only
- **AND** it does not pin album detail above the browser

#### Scenario: Non-Music library at wide width

- **WHEN** another hero-bearing library meets the shared wide geometry conditions
- **THEN** it also renders Wide hero with a one-column left-rail browser

### Requirement: Wide left pane persistently shows album detail and tracks

The wide grouped Music right pane SHALL show the selected album's title, metadata, large artwork, and track list. The track list SHALL remain visible whether album browsing or track selection has focus. Artwork SHALL yield vertical space before the track list disappears, and a present track list SHALL retain a visible track viewport whenever the content height can fit one.

#### Scenario: Album browsing is active

- **WHEN** an album is selected in the wide left rail and track selection is inactive
- **THEN** the right pane shows that album's large hero treatment and a readable, non-cursor track preview

#### Scenario: Selected album changes

- **WHEN** the album cursor moves to another album
- **THEN** the right-pane title, metadata, artwork, loading state, and tracks update to the newly selected album without showing tracks from the previous album under the new title

#### Scenario: Album tracks are loading

- **WHEN** the selected wide-mode album's tracks are not cached yet
- **THEN** the right track region shows a loading state and replaces it with that album's tracks when available

#### Scenario: Content height is constrained

- **WHEN** the wide layout has limited vertical space
- **THEN** the artwork shrinks before the persistent track region is removed

### Requirement: Wide album browser occupies the right rail

In the wide grouped Music composition, the music-group pills SHALL render at the top of the left rail and the artist-grouped album browser SHALL render below them. Albums SHALL render one per row regardless of available left-rail width. Artist headers SHALL span the rail as non-selectable grouping labels.

#### Scenario: Wide grouped Music renders

- **WHEN** grouped Music uses the horizontal composition
- **THEN** the left rail shows group pills followed by a one-column artist-grouped album list

#### Scenario: Artist group contains several albums

- **WHEN** an artist group is visible in the wide left rail
- **THEN** each album occupies its own row beneath the artist header

#### Scenario: Group pill changes selection

- **WHEN** the user selects another music-group pill in wide mode
- **THEN** the left rail loads that group's albums, returns focus to album browsing, and the right pane follows the resulting album selection

### Requirement: Wide hero uses one focus treatment

The Wide hero arrangement SHALL apply one focused and unfocused surface treatment to every screen that uses it, including grouped Music and Home. During album browsing the list pane SHALL carry the focused treatment and the hero pane SHALL carry the resting treatment. During track selection those treatments SHALL reverse. When the Library panel itself is unfocused, both panes SHALL use the unfocused treatment. Grouped Music SHALL NOT define these colours itself.

#### Scenario: Album browser has focus

- **WHEN** track selection is inactive and the Library panel is focused
- **THEN** the list pane has the arrangement's focused treatment and the hero pane remains a readable preview

#### Scenario: Track selection has focus

- **WHEN** track selection is active and the Library panel is focused
- **THEN** the hero pane has the arrangement's focused treatment and the list pane is visibly dimmed while retaining the selected album marker

#### Scenario: Queue has focus

- **WHEN** the Queue panel has focus
- **THEN** both Music panes use the arrangement's unfocused treatment

#### Scenario: The focused treatment is changed

- **WHEN** the Wide hero focused treatment is changed in its single definition
- **THEN** grouped Music, Home, and audiobooks all render the change

### Requirement: Wide tracks support direct mouse interaction

Each visible wide-mode track SHALL have a logical mouse target covering all of its wrapped physical rows. A single click SHALL select that track and activate track selection. A double-click SHALL select and play that track. Clicking an album or music-group pill SHALL clear track selection and return focus to the left rail. Artwork and blank hero space SHALL NOT activate track selection or playback.

#### Scenario: Click a visible track

- **WHEN** the user single-clicks any visible physical row belonging to a track
- **THEN** that logical track becomes selected and visual focus shifts right

#### Scenario: Double-click a visible track

- **WHEN** the user double-clicks any visible physical row belonging to a track
- **THEN** that track becomes selected and playback starts from it

#### Scenario: Click an album while tracks have focus

- **WHEN** the user single-clicks an album in the left rail during track selection
- **THEN** track selection clears, that album becomes selected, and visual focus returns left

#### Scenario: Click artwork

- **WHEN** the user clicks album artwork or blank space in the wide right hero
- **THEN** no track is selected and no playback action is invoked

### Requirement: Grouped Music pre-warms neighbour album artwork

While grouped Music is visible and image fetching is idle-gated open, the system SHALL initiate artwork fetches for the albums neighbouring the painted album cursor in display order: up to one behind and up to three ahead, skipping the selected album itself (its artwork fetch is already covered by painting). This SHALL apply in both the narrow Inline hero presentation and the Wide hero presentation. The neighbour window SHALL be keyed off the cursor and display order actually being painted, not a separately resolved cursor. While the search-results grid is active, neighbour prefetch SHALL NOT fire (the grid is not the canonical album rail).

#### Scenario: Scrolling narrow grouped albums warms neighbours

- **WHEN** the user moves the album cursor in the narrow grouped Music view while image fetches are idle-allowed
- **THEN** artwork fetches are initiated for the neighbouring albums in the ±3-ahead/±1-behind display-order window around the painted cursor

#### Scenario: Scrolling the wide right rail warms neighbours

- **WHEN** the user moves the album cursor in the wide grouped Music left rail while image fetches are idle-allowed
- **THEN** artwork fetches are initiated for the neighbouring albums in the same display-order window around the painted cursor

#### Scenario: Rapid navigation suppresses prefetch

- **WHEN** the user is actively navigating (image fetches are idle-gated closed)
- **THEN** no neighbour artwork fetches are initiated

#### Scenario: Search grid suppresses prefetch

- **WHEN** the grouped Music search-results grid is active
- **THEN** no neighbour album-artwork prefetch is initiated for the underlying album order

## RENAMED Requirements

- FROM: `Wide left pane persistently shows album detail and tracks`
- TO: `Wide right pane persistently shows album detail and tracks`

- FROM: `Wide album browser occupies the right rail`
- TO: `Wide album browser occupies the left rail`

- FROM: `Hero-on-left uses one focus treatment`
- TO: `Wide hero uses one focus treatment`
