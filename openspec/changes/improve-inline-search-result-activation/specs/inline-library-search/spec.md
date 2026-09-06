## MODIFIED Requirements

### Requirement: Results are navigable and activatable without leaving search

While the input box is open, Up and Down SHALL move the selection through the result list, the page keys SHALL move it by one viewport, and Home and End SHALL jump to the first and last result. Cursor movement SHALL NOT alter the query, and typing SHALL NOT alter the cursor beyond the reset that a query change causes.

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
