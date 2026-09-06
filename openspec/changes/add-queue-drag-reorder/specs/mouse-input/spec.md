## MODIFIED Requirements

### Requirement: Pointer gestures are recognized by the mounted parent

Each mounted destination parent SHALL recognize click, double-click, right-click,
wheel, and drag gestures from the raw mouse events it receives, using a private
`MouseGestureState`. The double-click interval and wheel throttle SHALL NOT be
held as shell-global state keyed by screen position. An embedded canonical
media-list control SHALL NOT recognize gestures — it only resolves a point
within the list rectangle its parent painted to a stable target, and the parent
delegates list-point resolution to it.

A drag SHALL be recognized as a left-button press that arms a drag anchor at the
press position, followed by pointer motion while the button is held, and ended
by the button release. The recognizer SHALL report the anchor position and the
current pointer position with each motion, and SHALL report the end of the drag
so the parent can release any state it holds for it. Recognizing a drag SHALL
NOT suppress the click the press already produced: a press remains a click, and
a drag is an additional gesture that follows it. A press that is released
without intervening motion SHALL produce no drag gesture at all.

Drag anchor state SHALL be private to the recognizing parent, in the same way
the double-click interval and wheel throttle are. A parent that does not
interpret drag SHALL be unaffected by the gesture's existence.

Hit geometry for a uniform row flow SHALL be resolved from the flow the control
already exports to its painter, not from a separately stored per-row rectangle
list. A stored rectangle registry is for irregular painted controls — pills,
scope buttons, transport controls, group selectors, overlay rows — whose owner
populates it in the same code that paints those rectangles.

A mounted parent SHALL translate a recognized gesture into a semantic typed `Msg`
carrying the resolved target (a child-returned row identity, a control, a pill
index, a seek fraction), never raw coordinates for the shell to re-resolve. The
shell handler for that `Msg` SHALL accept the resolved target as an argument, and
SHALL NOT read the painted geometry of the component that emitted it. A drag
gesture SHALL be translated the same way: the parent resolves both the anchor
and the current position to stable targets before emitting, and SHALL NOT emit
positions for the shell to resolve.

The gesture vocabulary SHALL remain open to hover (`enter`, `leave`) gestures
without changing the delivery or arbitration mechanism; those gestures are out
of scope for this capability but SHALL NOT be precluded by its design.

#### Scenario: A double-click activates the pointed row

- **WHEN** the user clicks the same row twice within the double-click interval
- **THEN** the mounted parent recognizes a double-click, delegates row resolution
  to the embedded list control, and emits the activation intent for that row's
  child-returned identity
- **AND** a single click on the same row emits only a focus/selection intent

#### Scenario: A wheel event scrolls the pointed list

- **WHEN** the user turns the wheel over a scrollable canonical list in any panel
- **THEN** the mounted parent recognizes the scroll gesture, subject to its own
  throttle, and the embedded list control scrolls its own viewport and keeps its
  own row identity, whether or not the list holds keyboard focus

#### Scenario: A right-click opens the context menu at the pointer

- **WHEN** the user right-clicks a selectable row on any migrated interactive
  surface that paints selectable rows
- **THEN** the row is focused and the context menu opens anchored at the click
  position

#### Scenario: A press and drag is recognized as a drag

- **WHEN** the user presses the left button over a surface and moves the pointer
  while holding it
- **THEN** the parent recognizes a click at the press position, and then a drag
  gesture for each motion, carrying both the press position and the current
  position
- **AND** releasing the button ends the drag

#### Scenario: A press without motion is only a click

- **WHEN** the user presses and releases the left button without moving the
  pointer
- **THEN** the parent recognizes a click and no drag gesture

#### Scenario: A parent that does not interpret drag is unaffected

- **WHEN** the user drags over a surface whose parent has no drag behaviour
- **THEN** the surface behaves exactly as it did before drag recognition existed
