# feeds-service-wide-list Specification

## Purpose
Defines the Feeds Service's one-column Wide layout, semantic rail framing, and state-bearing row geometry.

## Requirements

### Requirement: Wide Feeds Service uses one column
The Feeds Service/tab Wide panel MUST render one full-width selectable row per FeedEntry at and above the Wide breakpoint, including exactly at 82 columns. It MUST NOT use `library_column_count` to create a second column.

#### Scenario: threshold width
- **GIVEN** the Feeds tab is selected and the content width is 82 columns
- **WHEN** Wide content is rendered
- **THEN** every visible FeedEntry occupies one row spanning the list width and no second-column cell is emitted

### Requirement: Wide rail has established semantic framing
The Wide Feeds Service left rail MUST paint the existing semantic surface/backdrop and border treatment used by the Wide hero list panel. The treatment MUST be applied by the arrangement/render boundary, not by arbitrary screen colors. Its semantic border/background MUST NOT overwrite the first visible heading or the final visible FeedEntry/marker at the 82-column threshold or a larger Wide width.

#### Scenario: framed rail
- **GIVEN** a Wide Feeds Service panel with visible entries
- **WHEN** its buffer is rendered
- **THEN** the rail background and border cells match the established semantic roles and remain present behind the list

### Requirement: Selected rows have coherent one-column geometry
A selected Feeds Service row MUST paint its title exactly once, with its selected background and markers aligned to that row's full-width geometry. Active and played markers MUST NOT be repeated or positioned as if rows occupy multiple columns.

#### Scenario: selected metadata entry
- **GIVEN** a metadata-bearing selected FeedEntry at the Wide threshold
- **WHEN** the row is rendered
- **THEN** its title appears once, its selected background is contiguous across the row, and each marker is aligned within that row

#### Scenario: framing preserves edge content at Wide widths
- **GIVEN** a metadata-bearing Feeds Service list whose first visible row is a heading and whose final visible row is a state-bearing FeedEntry/marker
- **WHEN** the buffer is rendered at 82 columns and at a larger Wide width
- **THEN** the first heading remains readable and the final visible FeedEntry/marker retains its semantic background and marker
- **AND** the semantic border/background cells do not overwrite either content row

### Requirement: Narrow behavior is preserved
The change MUST preserve existing Narrow Feeds Service output and geometry. Any Narrow change requires a failing regression test demonstrating necessity.

#### Scenario: narrow regression
- **GIVEN** the existing Narrow Feeds fixture
- **WHEN** it is rendered before and after the Wide correction
- **THEN** its established output and row geometry remain unchanged

### Requirement: Verification is non-vacuous
Automated coverage MUST exercise selected, played, and active states with metadata-bearing FeedEntry fixtures at width 82 and a larger Wide width, and MUST assert rendered geometry/output for the three defects.

#### Scenario: state-bearing threshold matrix
- **GIVEN** representative metadata-bearing FeedEntries in selected, played, and active states
- **WHEN** rendered at 82 columns and a larger Wide width
- **THEN** tests verify one-column placement, semantic framing, title count, marker alignment, and preservation of the first visible heading and final visible FeedEntry/marker from semantic border/background overwrite
