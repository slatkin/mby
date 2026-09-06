# library-list-columns Specification

## Purpose
TBD - created by archiving change two-column-library-list. Update Purpose after archive.

## Requirements

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

#### Scenario: Wide hero list stays single-column
- **WHEN** a Wide hero browser is at or above the shared breakpoint
- **THEN** its left rail SHALL render a single column

### Requirement: Items flow row-major across columns

The library list SHALL place items in row-major order, so that consecutive items occupy consecutive cells left to right before wrapping to the next row. An item's position SHALL NOT depend on the viewport height. A selected-row replacement SHALL occupy the selected item's flow position and SHALL be budgeted once.

#### Scenario: Row-major placement

- **WHEN** the list renders items in two columns
- **THEN** the first item SHALL occupy the leftmost cell of the first row, the second item the rightmost cell of the first row, the third item the leftmost cell of the second row, and so on

#### Scenario: Viewport height change

- **WHEN** the terminal height changes without changing the list pane width
- **THEN** each item SHALL remain in the same column and the same relative row as before

#### Scenario: Trailing partial row

- **WHEN** the number of items in a row-major run is not a multiple of the column count
- **THEN** the final row SHALL be partially filled and the empty cells SHALL render as blank list background

### Requirement: Scrolling remains continuous and row-based

The library list SHALL continue to scroll by display rows using a stored scroll offset, and SHALL NOT introduce a paged scrolling model. Moving the cursor SHALL bring it into view by adjusting the scroll offset rather than by jumping to a page boundary.

#### Scenario: Scrolling in two-column mode

- **WHEN** the user scrolls a two-column list
- **THEN** the content SHALL move by whole display rows and no item SHALL change column as a result of scrolling

#### Scenario: Cursor moved out of view

- **WHEN** cursor movement places the selection outside the visible range
- **THEN** the scroll offset SHALL adjust by the minimum number of rows needed to bring the selected block into view

### Requirement: Full-width rows span all columns

Letter headers, the inline movie banner, and inline series detail SHALL render as full-width rows spanning every column. Selected detail SHALL replace the item row that contains the cursor and SHALL NOT leave a duplicate or blank selected row before the detail.

#### Scenario: Selected item with inline banner

- **WHEN** an item with an inline banner is selected in two-column mode
- **THEN** the banner SHALL render as full-width rows at the selected item's flow position, and the other item on a packed row SHALL remain in place

#### Scenario: Cursor moved to an adjacent item

- **WHEN** the cursor moves from one item to another
- **THEN** no item SHALL change its column as a result of the detail filler moving

#### Scenario: Letter header

- **WHEN** the list is letter-grouped and rendered in two columns
- **THEN** each letter header SHALL occupy its own full-width row above that letter's items

### Requirement: Letter buckets pack independently

When the list is letter-grouped, each bucket SHALL begin on a fresh item row. An item row SHALL NOT contain items from two different buckets.

#### Scenario: Bucket with an odd item count

- **WHEN** a letter bucket contains an odd number of items in two-column mode
- **THEN** that bucket's final row SHALL leave its trailing cell empty and the next bucket's header and items SHALL begin on subsequent rows

### Requirement: The selected block renders as a tab joined to its panel

When a selected item has inline detail, the selected block background SHALL cover the selected cell's slot across the item row and its top padding row, and SHALL cover the full pane width across the detail rows, with no gap or seam between the two regions. The cell sharing the selected item's row SHALL retain the ordinary list background. The block SHALL use the existing focused and unfocused background colors.

#### Scenario: Selected item in the left column

- **WHEN** an item in the left column is selected and has inline detail
- **THEN** the selected block background SHALL cover only the left cell's slot on the item and top padding rows, and the full pane width on the detail rows below

#### Scenario: Selected item in the right column

- **WHEN** an item in the right column is selected and has inline detail
- **THEN** the selected block background SHALL cover only the right cell's slot on the item and top padding rows, and the full pane width on the detail rows below

#### Scenario: Single-column mode

- **WHEN** the list renders in a single column
- **THEN** the selected block SHALL be full width on every row, matching the current rectangular appearance

#### Scenario: Unfocused list

- **WHEN** the library list is not focused
- **THEN** the tab and panel regions SHALL both use the unfocused background color

### Requirement: Cursor movement accounts for columns

Left and right SHALL move the cursor by one item. Up and down SHALL move the cursor by one item row. Page up and page down SHALL move by one viewport of item rows. Home and end SHALL continue to select the first and last item. The cursor SHALL remain a single item index.

#### Scenario: Horizontal movement within a row

- **WHEN** the cursor is on the left cell of a row in two-column mode and the user presses right
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

### Requirement: Selection survives a column count change

Changing the column count SHALL preserve the selected item. The list SHALL re-lay out around the same item rather than resetting the cursor or selecting a different item.

#### Scenario: Resize from one column to two

- **WHEN** the list pane widens past the two-column threshold while an item is selected
- **THEN** the same item SHALL remain selected and SHALL be scrolled into view in the new layout

#### Scenario: Resize from two columns to one

- **WHEN** the list pane narrows below the two-column threshold while an item is selected
- **THEN** the same item SHALL remain selected and SHALL be scrolled into view in the new layout

### Requirement: Scroll indicator reflects the row-based layout

The list scroll indicator SHALL be computed from the display row count and scroll offset of the laid-out list, so that in two-column mode it reflects the reduced number of rows rather than the item count.

#### Scenario: Two-column scroll indicator

- **WHEN** a list of items that requires scrolling in one column fits without scrolling in two columns
- **THEN** the scroll indicator SHALL not be shown
