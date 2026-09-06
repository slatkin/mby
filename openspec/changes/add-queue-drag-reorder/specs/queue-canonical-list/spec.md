## ADDED Requirements

### Requirement: Queue rows reorder by pointer drag

The Queue SHALL support reordering a queue entry by dragging it with the
pointer, as the pointer equivalent of its keyboard move action.

Pressing the left button on a Queue row SHALL select that row and grab it.
While the button is held, whenever the pointer moves onto a different Queue row,
the grabbed entry SHALL move to that row's position in the displayed queue, so
the grabbed entry visibly travels with the pointer. Releasing the button SHALL
release the grab and leave the entry where it lies.

A drag SHALL carry the grabbed entry's stable slot identity and the stable slot
identity of the row it is dropped onto. The Queue SHALL NOT emit a snapshot row
index for a drag, because the queue can be reordered by the playback owner
between paint and dispatch.

A pointer position that resolves to no Queue row — outside the list, past the
last entry, or on a non-selectable row — SHALL leave the order unchanged for as
long as the pointer stays there; the grab SHALL remain armed so that returning
to a row resumes the drag.

Each reorder a drag performs SHALL be individually undoable, on the same undo
path as a keyboard move.

Reordering the Queue by drag SHALL apply the same effects as reordering it by
keyboard: the playing entry's position SHALL be corrected when the move
displaces it, the queue SHALL be persisted, and a controlling playback owner
SHALL be told to make the same move addressed by slot identity.

#### Scenario: Dragging an entry over another row reorders it

- **WHEN** the user presses the left button on a Queue entry and moves the
  pointer onto a different Queue row
- **THEN** the grabbed entry moves to that row's position
- **AND** the entry remains selected

#### Scenario: A drag across several rows reorders once per row crossed

- **WHEN** the user drags a Queue entry across several rows before releasing
- **THEN** the entry moves once for each row it crosses onto
- **AND** the final order is the same as if it had been moved that many single
  steps by keyboard

#### Scenario: Dragging past the end of the list changes nothing

- **WHEN** the user drags a Queue entry onto blank space below the last entry
- **THEN** the queue order is unchanged
- **AND** moving the pointer back onto a row resumes reordering

#### Scenario: A drag reorder is undoable

- **WHEN** the user drags a Queue entry one row and then requests undo
- **THEN** the entry returns to the position it was dragged from

#### Scenario: Dragging the playing entry keeps playback on it

- **WHEN** the user drags the currently playing Queue entry to a new position
- **THEN** playback continues on that entry uninterrupted
- **AND** the playing position tracks the entry's new index
