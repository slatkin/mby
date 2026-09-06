# audiobookshelf-podcast-browsing Specification

## Purpose
Defines read-only discovery and browsing of Audiobookshelf podcast libraries, shows, downloaded episodes, progress, artwork, and personalized shelves before Audiobookshelf playback is introduced.
## Requirements
### Requirement: Ready Audiobookshelf discovers accessible podcast libraries
After Audiobookshelf becomes Ready, mbv SHALL discover the authenticated user's accessible Audiobookshelf libraries using the Audiobookshelf 2.36 API contract. It SHALL expose podcast libraries for browsing through this capability and book libraries for browsing through the `audiobookshelf-book-browsing` capability.

#### Scenario: User has accessible podcast libraries
- **WHEN** Audiobookshelf becomes Ready for a user with one or more accessible podcast libraries
- **THEN** mbv SHALL load those podcast libraries without waiting for Emby or Feeds
- **THEN** each discovered podcast library SHALL become available as a content tab

#### Scenario: User has only audiobook libraries
- **WHEN** Audiobookshelf becomes Ready for a user whose accessible libraries are all book libraries
- **THEN** Audiobookshelf SHALL remain Ready
- **THEN** mbv SHALL add a content tab for each book library through the `audiobookshelf-book-browsing` capability rather than adding no tab

#### Scenario: Audiobookshelf is the only configured content Service
- **WHEN** mbv starts with configured Audiobookshelf content and no configured Emby Service or feed subscriptions
- **THEN** mbv SHALL enter its ordinary content UI rather than opening Services settings as though no content Service were configured
- **THEN** Audiobookshelf initialization and discovery SHALL occur for both bare-mode and attached Local daemon clients

#### Scenario: Catalog request explicitly rejects the credential
- **WHEN** an authenticated catalog request explicitly rejects the persisted Audiobookshelf credential
- **THEN** Audiobookshelf SHALL enter Needs authentication through the existing Service lifecycle
- **THEN** mbv SHALL remove its Audiobookshelf tabs and catalog content while preserving non-secret setup

#### Scenario: Catalog request is unavailable or incompatible
- **WHEN** library discovery cannot complete because the server is unavailable or does not satisfy the required Audiobookshelf 2.36 contract
- **THEN** mbv SHALL present Audiobookshelf as unavailable with a concise retryable or compatibility result
- **THEN** mbv SHALL preserve the configured setup and credential and SHALL NOT use an older-server fallback

### Requirement: Podcast libraries are peer tabs with provider-specific behavior
Each accessible Audiobookshelf podcast library SHALL appear as a peer tab alongside Home, Emby libraries, and Feeds. Selecting an Audiobookshelf tab SHALL dispatch only Audiobookshelf browsing behavior and SHALL NOT fall through to Emby library actions.

#### Scenario: User switches among content tabs
- **WHEN** the user navigates across Home, Emby library, Audiobookshelf podcast library, and Feeds tabs that are present
- **THEN** each tab SHALL retain its correct identity, title, and provider-specific selection state
- **THEN** tab navigation by keyboard or mouse SHALL select the same ordered destination

#### Scenario: User invokes an Emby-specific action from an Audiobookshelf tab
- **WHEN** an Audiobookshelf podcast library is selected
- **THEN** Emby-specific playlist, watched-state, shuffle, route, search, and context-menu actions SHALL NOT operate on the Audiobookshelf selection

#### Scenario: Podcast library is loading or empty
- **WHEN** an Audiobookshelf tab has not finished loading shows or contains no shows
- **THEN** mbv SHALL render a provider-specific loading, error, or empty state without indexing an Emby library

### Requirement: Podcast shows load incrementally with stable selection
mbv SHALL list podcast shows from the selected Audiobookshelf library using bounded pagination. Show identity SHALL be the Audiobookshelf Service kind plus `libraryItemId`, and refresh or page loading SHALL preserve the selected show when that identity remains present.

#### Scenario: User reaches the loaded page boundary
- **WHEN** more podcast shows are available beyond the currently loaded page and navigation approaches the boundary
- **THEN** mbv SHALL request the next bounded page and append each show at most once
- **THEN** existing shows SHALL remain navigable while the request is pending

#### Scenario: Show list refresh retains the selected show
- **WHEN** the show list refreshes and the selected `libraryItemId` remains in the result
- **THEN** mbv SHALL restore selection to that show regardless of its new positional index

#### Scenario: Show list refresh removes the selected show
- **WHEN** the show list refreshes and the selected `libraryItemId` is no longer present
- **THEN** mbv SHALL select the nearest valid show or the library's empty state

### Requirement: Podcast libraries use the shared responsive hero presentation
An Audiobookshelf podcast library SHALL use Wide hero when the shared wide geometry conditions fit and selected-row replacement otherwise. Wide detail occupies the right workspace beside a single-column show browser; inline detail replaces the selected show row in one scrolling column. The podcast tab SHALL not reserve a separate detail block or define a surface-specific geometry rule.

The following substitutions SHALL be the only domain changes to that composition:

| TV Shows tab | Audiobookshelf podcast tab |
|---|---|
| Series | Podcast show |
| Series Primary image | Audiobookshelf podcast cover |
| Season selector | `All` / `Played` / `Unplayed` filter selector |
| Episodes in the selected season | Downloaded episodes matching the selected filter |

All other observable layout behavior SHALL match the TV Shows tab, including the hero shell and content padding, image slot, row budgeting, list column count, selected-cell treatment, focus styling, scrolling, and loading placeholder stability.

#### Scenario: Podcast library is displayed
- **WHEN** an Audiobookshelf podcast library and a TV Shows library are displayed at the same terminal dimensions and image setting
- **THEN** both tabs SHALL use the same shared wide or inline presentation for their available geometry
- **THEN** the podcast tab SHALL render podcast shows in the browser positions occupied by Series rows in the TV Shows tab
- **THEN** wide podcast detail SHALL occupy the right workspace beside the single-column browser

#### Scenario: Podcast selection changes
- **WHEN** the user moves selection between podcast shows
- **THEN** the hero or replacement detail SHALL update to the newly selected podcast
- **THEN** the show list SHALL retain provider-native selection identity across loaded-page changes

#### Scenario: Selected show scrolls outside the visible list rows
- **WHEN** the selected podcast's row is outside the visible portion of the lower show list
- **THEN** inline scrolling SHALL keep the selected show and its replacement detail addressable together

#### Scenario: Terminal width crosses the TV list column breakpoint
- **WHEN** the podcast tab crosses a width at which the TV Shows tab changes between one and two list columns
- **THEN** the podcast tab SHALL switch between Wide hero and selected-row replacement at the shared boundary

#### Scenario: Terminal height cannot fit the hero
- **WHEN** the TV Shows tab would suppress its hero because the available height cannot fit the minimum hero and a usable list
- **THEN** the podcast tab SHALL use selected-row replacement and restore the ordinary selected row if detail cannot fit

### Requirement: The selected podcast hero uses Audiobookshelf cover artwork
The selected podcast hero SHALL place the selected podcast's Audiobookshelf cover in the same right-aligned image slot, with the same dimensions, scaling, text wrapping, loading treatment, and images-disabled behavior as the selected Series Primary image in the TV Shows hero. The cover SHALL be fetched from the configured Audiobookshelf Service using the selected podcast's provider-native library item identity.

Podcast title and available author metadata SHALL occupy the corresponding TV hero text area. Missing metadata SHALL collapse without moving the image or changing the TV hero's structural rules.

#### Scenario: Selected podcast has a cover
- **WHEN** images are enabled and the selected podcast has an Audiobookshelf cover
- **THEN** that cover SHALL be fetched and rendered in the TV Series image position within selected detail
- **THEN** the cover SHALL NOT be rendered as a thumbnail in the lower show list

#### Scenario: Selected podcast cover is loading
- **WHEN** images are enabled and the selected podcast cover request is pending
- **THEN** the hero SHALL reserve and paint the same image placeholder area used while a TV Series image is loading

#### Scenario: Selected podcast has no usable cover
- **WHEN** images are enabled but the selected podcast has no usable cover
- **THEN** the hero SHALL follow the same missing-Primary-image behavior as the TV Shows hero without breaking its text, filter, or episode layout

#### Scenario: Images are disabled
- **WHEN** images are disabled
- **THEN** the podcast hero SHALL omit cover fetching and rendering
- **THEN** its text SHALL use the same image-disabled width and row budgeting as the TV Shows hero

### Requirement: Selected podcasts map TV season selection to played-state filters
The selected podcast hero SHALL expose exactly three episode filters: `All`, `Played`, and `Unplayed`. These filters SHALL occupy the same selector row and use the same pill appearance, overflow behavior, focus treatment, and selection-mode visibility as TV season selectors.

#### Scenario: Podcast show is selected but episode selection is inactive
- **WHEN** a podcast show is selected and the user has not entered episode-selection mode
- **THEN** the hero SHALL present the filter summary in the same state and position in which the TV hero presents its season summary
- **THEN** the episode rows SHALL have the same visibility as TV episode rows outside season-selection mode

#### Scenario: User enters episode selection
- **WHEN** the user activates the selected podcast show
- **THEN** the `All`, `Played`, and `Unplayed` pills SHALL become selectable in the TV season-selector position
- **THEN** focus SHALL enter the filtered episode rows using the same visual mode transition as the TV Shows tab

#### Scenario: Played and unplayed filters
- **WHEN** `Played` or `Unplayed` is selected
- **THEN** Played SHALL include only completed progress and Unplayed SHALL include missing or incomplete progress

#### Scenario: Filter changes
- **WHEN** the user changes the active episode filter using the controls corresponding to TV season navigation
- **THEN** the episode cursor SHALL reset to a valid visible episode
- **THEN** the selected podcast SHALL remain selected

### Requirement: Downloaded episodes use the TV episode-list presentation
Downloaded podcast episodes SHALL render in the same table area and with the same row height, marker position, title and duration column geometry, truncation, focused and unfocused colors, cursor styling, and available row budget as TV episodes. The podcast implementation SHALL substitute podcast-native episode data without converting it to an Emby item.

#### Scenario: Podcast has downloaded episodes
- **WHEN** the selected podcast has matching downloaded episodes and episode selection is active
- **THEN** the hero SHALL render one selectable TV-style episode row per matching episode with provider-native identities

#### Scenario: Podcast detail is empty or loading
- **WHEN** matching episodes are empty or detail is loading
- **THEN** the episode-table area SHALL show a scoped state without collapsing the hero or hiding the lower show list

#### Scenario: User changes shows while detail is loading
- **WHEN** an expanded-show result completes after the user has selected a different show
- **THEN** mbv SHALL NOT replace the currently displayed episode rows with the stale selection's episodes

### Requirement: Episode progress is read-only and identity-qualified
mbv SHALL display the authenticated user's Audiobookshelf progress for downloaded podcast episodes using `libraryItemId` and `episodeId`. Catalog browsing SHALL NOT write, infer, or periodically report progress.

#### Scenario: Episode has listening progress
- **WHEN** Audiobookshelf reports current time or completion state for a downloaded episode
- **THEN** mbv SHALL display the corresponding resume position or finished state on that episode

#### Scenario: Episode has no listening progress
- **WHEN** no progress record exists for a downloaded episode
- **THEN** mbv SHALL display it as unstarted rather than borrowing progress from another show or episode

#### Scenario: Progress changes outside mbv while the tab remains open
- **WHEN** progress changes on the server while the Audiobookshelf Socket.IO connection is authenticated and the tab remains open
- **THEN** mbv SHALL update the displayed progress for the matching episode from the resulting `user_item_progress_updated` event, without requiring an explicit REST refresh

#### Scenario: Progress changes while the socket is disconnected
- **WHEN** progress changes on the server while the Audiobookshelf Socket.IO connection is not currently authenticated
- **THEN** mbv MAY continue displaying the last REST-loaded value until the socket reconnects or an explicit REST refresh occurs

### Requirement: Podcast artwork is authenticated and Service-scoped
mbv SHALL fetch Audiobookshelf podcast artwork through the configured Service credential without exposing that credential in cache keys, logs, user-visible errors, or cross-Service state. Artwork state SHALL be isolated from Emby and from a replacement Audiobookshelf server.

#### Scenario: Show artwork is available
- **WHEN** a visible podcast show has authenticated cover artwork
- **THEN** mbv SHALL display it through the configured terminal image protocol and cache it under Service-qualified identity

#### Scenario: Artwork is absent or images are disabled
- **WHEN** a show has no cover or terminal images are disabled
- **THEN** the podcast browser SHALL remain fully usable with its text and placeholder presentation

#### Scenario: Audiobookshelf server is replaced
- **WHEN** the user confirms Audiobookshelf Service replacement
- **THEN** cached artwork belonging to the previous server SHALL NOT be displayed for items from the replacement server

### Requirement: Personalized shelves are absent from the podcast tab
The Audiobookshelf podcast tab SHALL NOT render or navigate personalized shelf data, and shelf data SHALL NOT affect show order, selection, scrolling, hit testing, or pagination.

#### Scenario: Catalog includes personalized shelves
- **WHEN** Audiobookshelf returns personalized shelf data
- **THEN** the top selected-podcast hero and lower podcast show list SHALL remain unaffected

### Requirement: Catalog results obey the current Service lifecycle
Every asynchronous Audiobookshelf catalog, detail, progress, shelf, and artwork result SHALL be reconciled with the Service setup generation that initiated it. Replacement, removal, authentication rejection, or a newer setup generation SHALL prevent old-server data from becoming visible.

#### Scenario: Stale result arrives after replacement
- **WHEN** a result initiated for the previous Audiobookshelf server arrives after Service replacement
- **THEN** mbv SHALL ignore it without changing current tabs, selection, progress, shelves, artwork, or Service state

#### Scenario: User removes Audiobookshelf
- **WHEN** Audiobookshelf Service removal is confirmed
- **THEN** mbv SHALL remove its podcast tabs and clear its in-memory catalog, progress, shelf, loading, and artwork state
- **THEN** Emby and Feeds content SHALL remain unaffected

### Requirement: Podcast activation starts supported local playback
Downloaded podcast episodes SHALL support ordinary play and enqueue activation through the Audiobookshelf podcast playback capability. Non-episode rows and unavailable episodes SHALL retain selection without queue or playback side effects.

#### Scenario: User plays a downloaded podcast episode
- **WHEN** the user invokes the ordinary play action on a selected downloaded episode
- **THEN** mbv SHALL submit that provider-native episode through the ordinary queue and owner-admission boundary

#### Scenario: User enqueues a downloaded podcast episode
- **WHEN** the user invokes the ordinary enqueue action on a selected downloaded episode
- **THEN** mbv SHALL add it to the selected Composed or eligible Bound queue without starting it

#### Scenario: User activates a non-episode row
- **WHEN** the selected Audiobookshelf row does not identify an available downloaded episode
- **THEN** mbv SHALL retain selection without creating a QueueItem or opening a playback session

### Requirement: Podcast browsing reaches playback only through explicit episode actions
Catalog discovery, pagination, detail loading, progress hydration, artwork, filtering, and navigation SHALL remain read-oriented and SHALL NOT themselves create queue items, resolve streams, or open playback sessions. Only an explicit play or enqueue action on a downloaded episode SHALL cross into the Audiobookshelf podcast playback capability.

#### Scenario: User browses podcast catalog surfaces
- **WHEN** the user discovers libraries, pages shows, expands episodes, views progress or artwork, changes filters, or moves selection
- **THEN** no Audiobookshelf media SHALL enter a Composed or Bound queue
- **THEN** no Audiobookshelf playback lifecycle request SHALL occur

#### Scenario: User explicitly submits an episode
- **WHEN** the user invokes play or enqueue on a selected downloaded episode
- **THEN** browsing SHALL provide its provider-native identity and snapshot metadata to the playback boundary
- **THEN** browsing state SHALL NOT receive or retain the Service credential, playback `sessionId`, resolved media URL, or request headers

### Requirement: Daemon-acknowledged progress reconciles client browse state
When an attached client applies a daemon owner's acknowledged Audiobookshelf progress event for the current setup generation, it SHALL update the displayed browse progress for the matching `libraryItemId` and `episodeId` and re-evaluate episode filters (such as Unplayed) accordingly, without polling, an explicit REST refresh, or Socket.IO. A superseded-generation event SHALL leave browse state unchanged.

#### Scenario: Acknowledged completion updates the Unplayed filter
- **WHEN** a capable client applies an acknowledged progress event marking a downloaded episode finished for the current generation
- **THEN** that episode SHALL present as finished and SHALL be excluded from the Unplayed filter

#### Scenario: Acknowledged position updates the resume state
- **WHEN** a capable client applies an acknowledged position below completion for the current generation
- **THEN** the matching episode SHALL display the corresponding resume position

#### Scenario: Superseded-generation acknowledgement is ignored for browse
- **WHEN** a received acknowledged progress event belongs to a replaced or removed setup generation
- **THEN** the client SHALL leave displayed browse progress and filters unchanged
