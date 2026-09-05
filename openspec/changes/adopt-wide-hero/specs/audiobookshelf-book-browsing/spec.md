## MODIFIED Requirements

### Requirement: Book libraries use the hero-on-left arrangement

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

#### Scenario: Shared hero-on-left presentation changes
- **WHEN** the Wide hero presentation changes
- **THEN** the wide book tab renders the change identically to grouped Music without an individual placement edit

#### Scenario: Terminal width crosses the two-column threshold
- **WHEN** the book tab crosses the shared width threshold
- **THEN** it recomputes Wide hero versus inline placement using the shared minimum-height guard

#### Scenario: Arrow focus leaves both panes visible
- **WHEN** the user changes pane focus in wide Audiobookshelf book browsing
- **THEN** the chapter workspace and book browser both remain visible

#### Scenario: The hero-on-left arrangement changes
- **WHEN** shared Wide hero geometry or styling changes
- **THEN** wide Audiobookshelf books inherit the change without local placement geometry

## RENAMED Requirements

- FROM: `Book libraries use the hero-on-left arrangement`
- TO: `Book libraries use the Wide hero arrangement`
