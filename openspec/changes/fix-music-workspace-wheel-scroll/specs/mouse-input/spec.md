## MODIFIED Requirements

### Requirement: Every migrated interactive surface has verified mouse parity

Every row in `docs/architecture/interactive-surface-ledger.md` SHALL record its
mouse ownership and the verification behind it, in the same way keyboard, state,
rendering, and geometry ownership are recorded. A row SHALL NOT be considered
complete while its mouse gestures are unverified.

For each surface the ledger SHALL state which component owns mouse hit-testing,
which gestures that surface supports, and the test or explicit manual validation
that confirms them. Panels SHALL support click-to-focus, click-to-select,
double-click-to-activate, wheel-scroll, and right-click-to-menu where the surface
has a corresponding keyboard action; overlays and popups SHALL support
click-to-select and click-to-dismiss where they have a corresponding keyboard
action. A surface with no meaningful pointer gesture SHALL say so explicitly.

A surface that renders at more than one breakpoint SHALL have its mouse
behaviour verified at each breakpoint it renders at, since its hit geometry
differs between them. A surface that exists at only one breakpoint SHALL record
which.

The Music workspace SHALL support wheel-scroll wherever it has a keyboard
scroll or paging action: wheel over the wide right rail SHALL page albums
(the PageUp/PageDown equivalent), wheel over the wide track table SHALL step
the track selection (the j/k equivalent), and wheel over the narrow album
list SHALL page albums.

#### Scenario: A surface renders at both breakpoints

- **WHEN** a surface paints pointable regions in both the wide and narrow
  arrangements
- **THEN** its ledger row records mouse verification for both
- **AND** a verification at one breakpoint alone does not satisfy the row

#### Scenario: The ledger is checked for mouse completeness

- **WHEN** the change that restores mouse support is complete
- **THEN** every ledger row has a filled mouse ownership/verification cell
- **AND** no row defers mouse verification to a later pass

#### Scenario: A surface gains a keyboard action after mouse restoration

- **WHEN** a new keyboard-driven action is added to a migrated interactive
  surface
- **THEN** the equivalent pointer gesture is added in the same change, or the
  ledger row records why the action has no pointer equivalent

#### Scenario: Wheel over the wide Music right rail

- **WHEN** the user turns the wheel over the wide Music workspace right rail
- **THEN** the album selection pages in the wheel direction, as PageUp/PageDown does

#### Scenario: Wheel over the wide Music track table

- **WHEN** the user turns the wheel over the wide Music workspace track table
- **THEN** the track selection steps in the wheel direction, as j/k does

#### Scenario: Wheel over the narrow Music album list

- **WHEN** the user turns the wheel over the narrow Music workspace album list
- **THEN** the album selection pages in the wheel direction
