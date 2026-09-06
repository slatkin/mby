## MODIFIED Requirements

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
