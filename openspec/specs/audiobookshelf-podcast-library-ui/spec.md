# audiobookshelf-podcast-library-ui Specification

## Purpose

Provides an Audiobookshelf podcast browsing experience whose presentation and interaction are structurally identical to the TV Shows tab, with podcast-native data substituted for TV-native data and without adding playback behavior.

## Requirements

### Requirement: The selected podcast hero uses Audiobookshelf cover artwork

The selected podcast hero SHALL place the selected podcast's Audiobookshelf cover in the same right-aligned image slot, with the same dimensions, scaling, text wrapping, loading treatment, and images-disabled behavior as the selected Series Primary image in the TV Shows hero. The cover SHALL be fetched from the configured Audiobookshelf Service using the selected podcast's provider-native library item identity.

Podcast title, author, and description SHALL occupy the corresponding TV hero text area as hero content lines. Missing metadata SHALL collapse without moving the image or changing the TV hero's structural rules. The hero SHALL show title, author, description, and cover image only — no filter pills and no episode rows.

#### Scenario: Selected podcast has a cover

- **WHEN** images are enabled and the selected podcast has an Audiobookshelf cover
- **THEN** that cover SHALL be fetched and rendered in the TV Series image position within selected detail
- **THEN** the cover SHALL NOT be rendered as a thumbnail in the lower show list

#### Scenario: Selected podcast cover is loading

- **WHEN** images are enabled and the selected podcast cover request is pending
- **THEN** the hero SHALL reserve and paint the same image placeholder area used while a TV Series image is loading

#### Scenario: Selected podcast has no usable cover

- **WHEN** images are enabled but the selected podcast has no usable cover
- **THEN** the hero SHALL follow the same missing-Primary-image behavior as the TV Shows hero without breaking its text layout

#### Scenario: Images are disabled

- **WHEN** images are disabled
- **THEN** the podcast hero SHALL omit cover fetching and rendering
- **THEN** its text SHALL use the same image-disabled width and row budgeting as the TV Shows hero

### Requirement: Personalized shelves are absent from the podcast tab

The Audiobookshelf podcast tab SHALL NOT render or navigate personalized shelf data, and shelf data SHALL NOT affect show order, selection, scrolling, hit testing, or pagination.

#### Scenario: Catalog includes personalized shelves

- **WHEN** Audiobookshelf returns personalized shelf data
- **THEN** the top selected-podcast hero and lower podcast show list SHALL remain unaffected

### Requirement: Podcast activation remains read-only

Activating a podcast show SHALL only enter the selection modal for episode browsing. Activating a podcast episode from the modal SHALL consume the activation without starting playback, enqueueing an item, opening a playback run or Session, or writing progress.

#### Scenario: User activates a podcast episode

- **WHEN** the user activates a selected podcast episode from the constituent-list modal
- **THEN** mbv SHALL retain selection without playback, queue, Session, or progress side effects

### Requirement: Podcast libraries use responsive hero presentations

An Audiobookshelf podcast library SHALL use the shared Wide hero presentation when it meets the wide geometry conditions and selected-row replacement otherwise. In Wide hero, the selected podcast's cover, metadata, and downloaded-episode workspace SHALL occupy the right pane while the single-column podcast-show browser occupies the left rail. In the replacement presentation, the same selected-show detail (title, author, description, cover) SHALL replace the active podcast-show row in list flow. The podcast tab SHALL obtain placement from the shared arrangement and SHALL NOT define a separate fallback.

The podcast tab SHALL supply podcast-native data without changing the shared placement rule: Podcast show for Series, Audiobookshelf cover for Series Primary image, and matching downloaded episodes for the selection modal. Image shape, metadata lines and order, colour variant, element presence, and image source MAY remain podcast-specific declarations.

#### Scenario: Podcast library is displayed wide

- **WHEN** an Audiobookshelf podcast library meets the shared wide geometry conditions
- **THEN** selected-show detail and downloaded episodes render in the right pane
- **AND** podcast shows render in the single-column left rail

#### Scenario: Podcast library is displayed narrow

- **WHEN** an Audiobookshelf podcast library does not meet the shared wide geometry conditions
- **THEN** podcast shows render in one scrolling column with alphabetical panel pills
- **AND** selected-show detail (title, author, description, cover) replaces the active show row
- **AND** no separate hero area is reserved above the show browser
- **AND** no episode rows or filter pills render inside the inline hero

#### Scenario: Podcast selection changes

- **WHEN** the user moves selection between podcast shows
- **THEN** the hero or detail workspace updates to the newly selected podcast
- **AND** the show list retains provider-native selection identity across loaded-page changes

#### Scenario: Selected show scrolls in the inline presentation

- **WHEN** the active podcast show moves through the narrow browser
- **THEN** scrolling keeps its media row and inline detail addressable together
- **AND** the replacement block owns the selected parent target while explicit child targets take precedence

#### Scenario: Terminal height cannot fit Wide hero

- **WHEN** the width meets the shared breakpoint but the minimum-height guard fails
- **THEN** the podcast tab uses selected-row replacement
- **AND** it restores the ordinary selected row if detail cannot fit

#### Scenario: Shared placement changes

- **WHEN** the shared Wide hero or inline presentation changes
- **THEN** the podcast tab renders the placement change without an individual geometry edit

#### Scenario: Podcast library is displayed

- **WHEN** an Audiobookshelf podcast library is displayed
- **THEN** it uses Wide hero when wide geometry fits and inline selected-show detail (title, author, description, cover) otherwise

#### Scenario: Selected show scrolls outside the visible list rows

- **WHEN** the selected show scrolls outside visible left-rail rows in Wide hero
- **THEN** the right workspace continues projecting that selected show

#### Scenario: Terminal width crosses the TV list column breakpoint

- **WHEN** the podcast tab crosses the shared width breakpoint
- **THEN** it recomputes Wide hero versus selected-row replacement rather than changing a detail layout column count

#### Scenario: Terminal height cannot fit the hero

- **WHEN** selected detail cannot fit with a usable active row
- **THEN** detail is suppressed and the browser retains the available area

#### Scenario: The retired separate placement changes

- **WHEN** the obsolete separate placement is removed
- **THEN** Audiobookshelf podcasts continue through only Wide hero and selected-row replacement

### Requirement: Podcast libraries use alphabetical panel pills

The Audiobookshelf podcast tab SHALL render alphabetical browsing pills (All, A–C, D–F, … `#`) in the panel area, one row, with the `⌘` prefix, matching every other library tab. The pills SHALL use the shared `render_pill_bar` widget and SHALL write `layout.selector_tabs`. Episode filter pills (All, Played, Unplayed) SHALL NOT render inside the inline hero. The played/unplayed filter SHALL live in the selection modal, not in the panel or the hero content.

#### Scenario: Podcast tab renders alphabetical pills

- **WHEN** the Audiobookshelf podcast tab is displayed with shows available
- **THEN** alphabetical browsing pills render in the panel area, one row, with the `⌘` prefix
- **AND** no pills render inside the inline hero

#### Scenario: Podcast tab pills use the shared widget

- **WHEN** the alphabetical pills are rendered
- **THEN** they use the same `render_pill_bar` widget and `PillBar` structure as every other library tab
- **AND** they write `layout.selector_tabs` for mouse hit-testing

### Requirement: Downloaded episodes use the selection modal

Downloaded podcast episodes SHALL be listed in the constituent-list modal (see `inline-hero-selection-modal`) when the user presses Enter on a selected podcast show. The modal SHALL render one selectable row per matching episode with the episode title and duration. The inline hero SHALL NOT render episode rows.

#### Scenario: User opens the episode modal

- **WHEN** the user presses Enter on a selected podcast show in the inline presentation
- **THEN** the constituent-list modal opens with matching downloaded episodes
- **AND** each episode shows its title and duration

#### Scenario: User selects an episode from the modal

- **WHEN** the user navigates to an episode in the modal and presses Enter
- **THEN** the episode is selected according to the podcast tab's existing activation behavior
- **AND** the modal closes

#### Scenario: Podcast detail is empty or loading

- **WHEN** matching episodes are empty or detail is loading
- **THEN** the modal shows a scoped empty or loading state
- **AND** the inline hero is unaffected
