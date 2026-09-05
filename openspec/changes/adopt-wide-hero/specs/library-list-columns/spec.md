## MODIFIED Requirements

### Requirement: Column count derives from the list pane width

The library list SHALL choose its column count from the width available to the list pane itself, not from terminal width, and SHALL use at most two columns. The shared responsive breakpoint SHALL NOT be derived from minimum cell width. A hero-bearing browse surface SHALL render a single-column browser in both presentations: in the left rail for Wide hero and in the full-width list for inline hero. A non-hero list MAY use two columns at or above the shared breakpoint and SHALL use one column below it.

#### Scenario: Wide hero-bearing browser
- **WHEN** a hero-bearing browse surface meets the wide geometry conditions
- **THEN** its left-rail browser SHALL render one column

#### Scenario: Inline hero browser
- **WHEN** a hero-bearing browse surface uses the inline presentation
- **THEN** its browser SHALL render one column
- **AND** the selected hero SHALL replace the active item row as one full-width flow segment

#### Scenario: Wide non-hero list pane
- **WHEN** a non-hero library list pane reaches the shared breakpoint
- **THEN** it MAY render two columns of items

#### Scenario: Narrow list pane
- **WHEN** any library list pane is below the shared breakpoint
- **THEN** the list SHALL render a single column

#### Scenario: Queue column resized or collapsed
- **WHEN** the queue column is widened, narrowed, or collapsed, changing the width available to the list pane
- **THEN** the active presentation and permitted column count SHALL be recomputed on the next frame

#### Scenario: The shared breakpoint is changed
- **WHEN** the shared breakpoint value is changed
- **THEN** every right-panel browse surface switches presentation at the new width without a surface-specific edit

#### Scenario: Wide list pane
- **WHEN** a non-hero list pane reaches the shared breakpoint
- **THEN** it MAY render two columns while hero-bearing browsers remain one column

#### Scenario: Hero-on-left list stays single-column
- **WHEN** a Wide hero browser is at or above the shared breakpoint
- **THEN** its left rail SHALL render a single column

### Requirement: Cursor movement accounts for columns

Left and right SHALL move the cursor by one item. Up and down SHALL move the cursor by one item row. Page up and page down SHALL move by one viewport of item rows. Home and end SHALL continue to select the first and last item. The cursor SHALL remain a single item index.

#### Scenario: Horizontal movement within a row

- **WHEN** the cursor is on the right cell of a row in two-column mode and the user presses right
- **THEN** the cursor SHALL move to the item in the right cell of that row

#### Scenario: Horizontal movement across a row boundary

- **WHEN** the cursor is on the last cell of a row and the user presses right
- **THEN** the cursor SHALL move to the first item of the next row

#### Scenario: Vertical movement

- **WHEN** the user presses down in two-column mode
- **THEN** the cursor SHALL move to the item directly below it, remaining in the same column where an item exists there

#### Scenario: Vertical movement past the end

- **WHEN** the user presses down from a cell in the second-to-last row and no item exists directly below
- **THEN** the cursor SHALL move to the last item rather than moving past the end of the list

#### Scenario: Paging

- **WHEN** the user presses page down in two-column mode
- **THEN** the cursor SHALL advance by the number of items contained in one viewport of item rows
