# audiobookshelf-book-browsing Specification

## Purpose

Defines read-only discovery and browsing of Audiobookshelf book libraries — library-to-tab exposure, author-surname grouping, responsive hero presentation, chapter display, and read-only progress — distinct from the podcast tab's TV-style browsing.

## Requirements

### Requirement: Book libraries are peer tabs with provider-specific behavior
Each accessible Audiobookshelf book library SHALL appear as a peer tab alongside Home, Emby libraries, Audiobookshelf podcast libraries, and Feeds, in the server's library order. Selecting a book tab SHALL dispatch only book browsing behavior and SHALL NOT fall through to Emby or Audiobookshelf podcast actions.

#### Scenario: Book and podcast libraries interleave in server order
- **WHEN** an Audiobookshelf server exposes both book and podcast libraries
- **THEN** mbv SHALL present their tabs in the order `/api/libraries` returns them
- **THEN** mbv SHALL NOT group or reorder tabs by `media_type`

#### Scenario: User invokes a podcast- or Emby-specific action from a book tab
- **WHEN** an Audiobookshelf book library is selected
- **THEN** podcast played-state filtering, playlist, watched-state, shuffle, route, search, and Emby context-menu actions SHALL NOT operate on the book selection

### Requirement: Books load incrementally, grouped and sorted by author surname
mbv SHALL list books from the selected Audiobookshelf book library using bounded pagination, grouped and sorted by author surname only, and further bucketed into alphabetical author-surname ranges (e.g. A-C, D-F) for pill-filtered browsing. Book identity SHALL be the Audiobookshelf Service kind plus `libraryItemId`, and refresh or page loading SHALL preserve the selected book when that identity remains present.

#### Scenario: Author surname determines sort position
- **WHEN** a book has one or more listed authors
- **THEN** mbv SHALL sort it using the first-listed author's surname, extracted from the raw author credit
- **THEN** remaining authors on a multi-author book SHALL NOT participate in the sort key

#### Scenario: Surname extraction fails
- **WHEN** author-name parsing cannot extract a surname from the raw credit
- **THEN** mbv SHALL fall back to the raw author credit string as the sort key
- **THEN** the book SHALL remain grouped and browsable rather than excluded

#### Scenario: Book list refresh retains the selected book
- **WHEN** the book list refreshes and the selected `libraryItemId` remains in the result
- **THEN** mbv SHALL restore selection to that book regardless of its new positional index or bucket

#### Scenario: Surname buckets omit empty ranges
- **WHEN** the sorted book list is grouped for browsing
- **THEN** mbv SHALL partition it into contiguous alphabetical author-surname ranges
- **THEN** a range with no books in the current library SHALL NOT produce an empty, selectable bucket

### Requirement: Book libraries use the Wide hero arrangement

An Audiobookshelf book library SHALL use Wide hero when it meets the shared wide geometry conditions, matching grouped Music: a persistent book hero and chapters pane beside a persistent single-column book browser with surname-bucket pills. Otherwise the selected book's hero and chapter detail SHALL replace the active book row in the single-column browser. Both the selected book and browser SHALL remain available in either presentation. The book tab SHALL obtain responsive placement from the shared arrangement and SHALL NOT evaluate the breakpoint, minimum-height guard, or a separate fallback itself.

The book tab SHALL supply book-native content and interaction state without defining placement geometry: Audiobookshelf cover, metadata and progress, chapter rows, surname-bucket pills, and existing pane-focus behavior.

#### Scenario: Terminal geometry crosses the wide boundary
- **WHEN** the book tab starts or stops meeting the shared wide geometry conditions
- **THEN** it switches between Wide hero and inline selected-book detail at the same boundary as every other hero-bearing browse surface

#### Scenario: Narrow selected book
- **WHEN** a book is selected in the inline presentation
- **THEN** that book's hero and chapter detail replace its active row in list flow
- **AND** other book rows remain part of the same single-column browser

#### Scenario: Hero follows the browser cursor
- **WHEN** the book browser cursor moves to another book
- **THEN** the hero and chapter detail update to that book without an Enter or open action
- **AND** the book browser remains visible

#### Scenario: A surname pill filters the browser
- **WHEN** the user selects an author-surname bucket pill
- **THEN** the browser contains only books in that bucket until another bucket is selected

#### Scenario: Arrow focus in wide presentation
- **WHEN** the user presses left or right while the wide book tab is focused
- **THEN** focus toggles between the chapter list and left-rail browser
- **AND** neither pane is hidden or replaced

#### Scenario: Shared Wide hero presentation changes
- **WHEN** the Wide hero presentation changes
- **THEN** the wide book tab renders the change identically to grouped Music without an individual placement edit

#### Scenario: Terminal width crosses the two-column threshold
- **WHEN** the book tab crosses the shared width threshold
- **THEN** it recomputes Wide hero versus inline placement using the shared minimum-height guard

#### Scenario: Arrow focus leaves both panes visible
- **WHEN** the user changes pane focus in wide Audiobookshelf book browsing
- **THEN** the chapter workspace and book browser both remain visible

#### Scenario: The Wide hero arrangement changes
- **WHEN** shared Wide hero geometry or styling changes
- **THEN** wide Audiobookshelf books inherit the change without local placement geometry

### Requirement: The selected book hero shows an inline progress percentage
The selected book hero SHALL place the selected book's Audiobookshelf cover in the same image slot as the Music hero's album cover, and SHALL show the book's listening progress as an inline `%` or `Finished` span in the hero meta, in the same style the podcast tab uses for episode progress. A resume-emphasizing hero treatment is out of scope for this capability.

#### Scenario: Selected book has listening progress
- **WHEN** the selected book has Audiobookshelf progress
- **THEN** the hero SHALL display the corresponding `%` or `Finished` span
- **THEN** the image, title, and author metadata positions SHALL remain unchanged by the presence of progress

#### Scenario: Selected book has no listening progress
- **WHEN** no progress record exists for the selected book
- **THEN** the hero SHALL display it as unstarted rather than borrowing progress from another book

### Requirement: Chapters render as first-class rows in the persistent list
The book tab's persistent list (the Music track list's analog) SHALL render one row per chapter from the selected book's Audiobookshelf `chapters[]`, using the book-relative chapter title and duration. Chapter rows SHALL use provider-native identity and SHALL NOT be converted to an Emby or podcast episode row shape. Chapter or audio-file detail SHALL be fetched as soon as the browser cursor moves onto a book, mirroring the Music tab's eager track fetch, rather than only after an explicit book-open action.

#### Scenario: Selected book has chapters
- **WHEN** the selected book has one or more chapters
- **THEN** mbv SHALL render each chapter as a selectable row in the persistent list area

#### Scenario: Selected book has no chapter metadata
- **WHEN** the selected book has no `chapters[]` entries
- **THEN** mbv SHALL render its `audioFiles` as the persistent list rows instead, without an empty or broken list state

#### Scenario: Cursor moves onto an uncached book
- **WHEN** the browser cursor moves onto a book whose chapter/audio-file detail is not yet cached
- **THEN** mbv SHALL fetch that detail immediately, without requiring an explicit book-open action
- **THEN** a fetch already in flight or cached for that book SHALL NOT be re-requested

### Requirement: Book progress is read-only and identity-qualified
mbv SHALL display the authenticated user's Audiobookshelf progress for a book using only `libraryItemId` — books have no episode identity. Catalog browsing SHALL NOT write, infer, or periodically report progress.

#### Scenario: Progress changes outside mbv while the tab remains open
- **WHEN** progress changes on the server without an explicit REST refresh
- **THEN** mbv MAY continue displaying the last REST-loaded value because live Socket.IO refresh is outside this capability

### Requirement: Book artwork is authenticated and Service-scoped
mbv SHALL fetch Audiobookshelf book cover artwork through the configured Service credential without exposing that credential in cache keys, logs, user-visible errors, or cross-Service state. Artwork state SHALL be isolated from Emby, from Audiobookshelf podcast artwork, and from a replacement Audiobookshelf server.

#### Scenario: Artwork is absent or images are disabled
- **WHEN** a book has no cover or terminal images are disabled
- **THEN** the book browser SHALL remain fully usable with its text and placeholder presentation

### Requirement: Catalog results obey the current Service lifecycle
Every asynchronous book catalog, chapter, progress, and artwork result SHALL be reconciled with the Service setup generation that initiated it. Replacement, removal, authentication rejection, or a newer setup generation SHALL prevent old-server data from becoming visible.

#### Scenario: Stale result arrives after replacement
- **WHEN** a result initiated for the previous Audiobookshelf server arrives after Service replacement
- **THEN** mbv SHALL ignore it without changing current tabs, selection, progress, or artwork

### Requirement: Book browsing reaches playback only through explicit book actions
Catalog discovery, pagination, chapter display, progress hydration, artwork, and navigation SHALL remain read-oriented and SHALL NOT themselves create queue items, resolve streams, or open playback sessions. Only an explicit play or enqueue action on a book, or a seek action on a chapter row of the active book, SHALL cross into the `audiobookshelf-book-playback` capability.

#### Scenario: User browses the book catalog
- **WHEN** the user discovers libraries, pages books, views chapters, progress, or artwork, or moves selection
- **THEN** no Audiobookshelf book SHALL enter a Composed or Bound queue
- **THEN** no Audiobookshelf playback lifecycle request SHALL occur
