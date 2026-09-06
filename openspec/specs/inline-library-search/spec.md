# inline-library-search Specification

## Purpose
Lets the user narrow the library list they are already looking at by typing a fuzzy query into a small input box above that list, so filtering a library never hides the library.
## Requirements
### Requirement: The search key opens an inline input box above the library list

Pressing the search key while a library tab is focused SHALL replace that library panel's browser pill row with a one-row Inline Search bar. The bar SHALL use the pill row's exact rectangle, background, and height.

The existing parent-background spacer below the pill row SHALL remain unchanged, and search results SHALL begin in the same content rectangle used by the normal library presentation. The search bar SHALL NOT overlay or dim the results or any other part of the view.

While Inline Search is active, the replaced pill controls SHALL NOT be painted or remain mouse-active. The active destination SHALL paint exactly one search bar and one result list; it SHALL NOT also paint a bordered search input or a second search presentation.

The search key SHALL have no effect on the home tab, which has no library list to filter.

The search key SHALL have no effect while the library panel is not the focused panel.

#### Scenario: Opening search on a library tab

- **WHEN** the user presses the search key with a library tab focused
- **THEN** a one-row Inline Search bar SHALL replace the browser pill row in its existing rectangle
- **AND** the parent-background spacer SHALL remain below the bar
- **AND** search results SHALL begin where the normal library content begins

#### Scenario: Pill controls while search is active

- **WHEN** Inline Search is active on a library panel
- **THEN** the panel's pill controls SHALL NOT be painted
- **AND** the pill controls SHALL NOT respond to mouse input

#### Scenario: One search presentation

- **WHEN** Inline Search is active in either Normal or Wide presentation
- **THEN** exactly one one-row search bar and one result list SHALL be painted by the active destination
- **AND** no bordered or duplicate search input SHALL be painted above the results

#### Scenario: Library list too short for the box

- **WHEN** the normal library-content rectangle has no rows available for results
- **THEN** the one-row search bar SHALL remain in the existing pill rectangle

#### Scenario: Search key on the home tab

- **WHEN** the user presses the search key on the home tab
- **THEN** nothing SHALL happen and no search bar SHALL appear

#### Scenario: Library panel is not focused

- **WHEN** the user presses the search key while the library panel is not focused
- **THEN** nothing SHALL happen and the pill row SHALL remain unchanged

### Requirement: Typing edits the query and re-filters the list in place

While the input box is open, printable characters SHALL be appended to the query and SHALL NOT be interpreted as library list shortcuts. Each change to the query SHALL re-score the corpus and replace the rendered list contents with the matches.

Matching SHALL be fuzzy, scored against each item's display name, and results SHALL be ordered by descending match score. An empty query SHALL show the whole corpus in its original order.

The selection SHALL reset to the first result whenever the query changes.

#### Scenario: Typing a query

- **WHEN** the user types characters into the open search box
- **THEN** the characters SHALL appear in the input box
- **AND** the list below SHALL show only items whose names fuzzy-match the query, ordered by descending score

#### Scenario: A list shortcut letter is typed

- **WHEN** the user types a character that is otherwise a library list shortcut
- **THEN** it SHALL be inserted into the query
- **AND** the library list action bound to that character SHALL NOT run

#### Scenario: Query emptied by deletion

- **WHEN** the user deletes back to an empty query without dismissing the search
- **THEN** the list SHALL show the whole corpus in its original order

### Requirement: The corpus spans the whole library, not the visible page

The corpus SHALL be the library's full item set, independent of lazy pagination and independent of any active letter-range filter. When the full set is not yet loaded at the moment search opens, the full-library fetch SHALL be started and the input box SHALL show a loading indicator until it completes.

A library configured for recursive album search SHALL use its album index as the corpus, matching against each album's indexed search text rather than its bare display name.

#### Scenario: Only part of the library has been paged in

- **WHEN** the user opens search on a library whose items are only partly loaded
- **THEN** the full item set SHALL be fetched
- **AND** the input box SHALL show a loading indicator until the fetch completes

#### Scenario: A letter-range filter is active

- **WHEN** the user opens search while a letter-range filter narrows the library view
- **THEN** the corpus SHALL span the entire library, not the filtered range

#### Scenario: Corpus still loading

- **WHEN** the query changes while the corpus fetch is still in flight
- **THEN** the view SHALL indicate that loading is in progress rather than presenting an empty result set as final

### Requirement: Results render as a flat list on every library type

While search is open, results SHALL render as a single flat list through the plain column-aware list renderer. No grouping applied by the underlying browse view — artist-grouped album headers or letter headers — SHALL be applied to search results.

Results SHALL NOT be reordered, mismatched, or omitted as a consequence of any grouping the browse view would otherwise apply. Every displayed row SHALL show the item that actually matched the query.

#### Scenario: Searching a grouped music library

- **WHEN** the user searches a music library whose browse view groups albums under artist headers
- **THEN** every album matching the query SHALL appear, ordered by match score
- **AND** each row SHALL display the album it matched, not a different album
- **AND** no artist headers SHALL be drawn

#### Scenario: Searching a letter-grouped library

- **WHEN** the user searches a library whose browse view groups items under letter headers
- **THEN** results SHALL render as a flat list with no letter headers

#### Scenario: Search dismissed on a grouped library

- **WHEN** the user dismisses search on a library whose browse view groups its items
- **THEN** the grouped presentation SHALL return unchanged

### Requirement: Results are navigable and activatable without leaving search

While the input box is open, Up and Down SHALL move the selection through the result list, the page keys SHALL move it by one viewport, and Home and End SHALL jump to the first and last result.

Cursor movement SHALL NOT alter the query, and typing SHALL NOT alter the cursor beyond the reset that a query change causes.

A result row reached by a mouse-down that began in the Inline Search bar SHALL retain its ordinary row context-menu actions and its Ctrl+P, Ctrl+S, and Ctrl+A shortcut actions.

Pressing Enter on a selected album result SHALL dismiss Inline Search, restore the standard library presentation, focus that album at its ordinary natural pill/list position, and enable that album's track-selection mode. Results that are not album results SHALL retain their existing activation behavior.

#### Scenario: Moving through results

- **WHEN** the user presses Down with results showing
- **THEN** the selection SHALL move to the next result
- **AND** the query SHALL be unchanged

#### Scenario: Result row actions after search-bar mouse-down

- **WHEN** the user presses the mouse in the Inline Search bar and then targets a result row
- **THEN** that row's context-menu actions and Ctrl+P, Ctrl+S, and Ctrl+A actions SHALL remain available

#### Scenario: Activating a result

- **WHEN** the user presses the activation key on a selected non-album result
- **THEN** the application SHALL act on that item as it would from the unfiltered library list

#### Scenario: Enter on an album result

- **WHEN** the user presses Enter on a selected album result
- **THEN** Inline Search SHALL close
- **AND** the standard library presentation SHALL focus that album in its ordinary natural pill/list position
- **AND** track-selection mode SHALL be enabled for that album

#### Scenario: Navigating an empty result set

- **WHEN** the query matches nothing and the user presses Up or Down
- **THEN** nothing SHALL happen and the search SHALL stay open

### Requirement: Open search survives responsive presentation transitions

An open Inline Search session SHALL remain open when the selected Emby destination changes between Normal and Wide presentation without changing destinations. The query and selected result SHALL be preserved, the same full-library corpus SHALL remain in effect, and the selected result SHALL remain visible after the results are laid out for the new presentation.

The input box and results SHALL move with the destination's library list; they SHALL NOT remain painted over the pane or area used by the previous presentation.

#### Scenario: TV search transitions from Normal to Wide

- **WHEN** Inline Search is open on a TV library and a resize changes the destination from Normal presentation to Wide presentation
- **THEN** Inline Search SHALL remain open in the Wide library-list pane
- **AND** its query and selected result SHALL be unchanged
- **AND** the selected result SHALL remain visible

#### Scenario: TV search transitions from Wide to Normal

- **WHEN** Inline Search is open on a TV library and a resize changes the destination from Wide presentation to Normal presentation
- **THEN** Inline Search SHALL remain open above the Normal library list
- **AND** its query and selected result SHALL be unchanged
- **AND** the selected result SHALL remain visible

#### Scenario: Search input follows its destination list

- **WHEN** an open Inline Search session crosses a responsive presentation transition
- **THEN** exactly one search input and one result list SHALL be painted in the current library-list area
- **AND** no search content SHALL be painted in the prior presentation's area

### Requirement: Dismissing search restores the unfiltered list

Pressing the dismiss key SHALL close the input box and restore the library list to its unfiltered contents, presentation, and prior navigation position. Pressing the delete key on an already-empty query SHALL dismiss the search the same way.

Dismissal SHALL discard the query and results; reopening search SHALL start from an empty query.

#### Scenario: Dismissing with the dismiss key

- **WHEN** the user presses the dismiss key while the search box is open
- **THEN** the box SHALL close
- **AND** the library list SHALL show its unfiltered items in their normal order and grouping

#### Scenario: Deleting past the start of the query

- **WHEN** the query is empty and the user presses the delete key
- **THEN** the search SHALL be dismissed

#### Scenario: Reopening after dismissal

- **WHEN** the user dismisses search and immediately reopens it
- **THEN** the query SHALL be empty

