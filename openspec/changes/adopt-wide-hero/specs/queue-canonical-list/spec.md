## MODIFIED Requirements

### Requirement: Queue composes canonical fixed-row mechanics

The Queue Interactive Component SHALL embed `WideMediaList<QueueSlotId>` directly for Queue's fixed-height rows. Queue SHALL NOT use `InlineMediaBrowser`, Wide hero, Inline hero, or responsive Wide/Inline handoff. Queue SHALL NOT duplicate selectable indexing, fixed-row placement, cursor movement, scrolling, or scrollbar geometry in the parent or shell. Every slot-targeted Queue effect request SHALL identify its stable `QueueSlotId`; only reorder MAY carry a destination position, and that position SHALL be resolved against the same canonical queue.

#### Scenario: Queue renders canonical rows

- **WHEN** Queue is visible in its supported panel mode
- **THEN** the canonical fixed-row child paints the Queue rows
- **AND** the Queue parent supplies prepared content and translates typed intents
- **AND** no legacy Queue body painter also paints that rect

#### Scenario: Queue state remains parent-owned where required

- **WHEN** the user changes Queue scope, reorders, activates, removes, or plays a slot
- **THEN** the Queue parent emits the corresponding typed request
- **AND** the shell retains Local/Remote scope, Player/queue authority, persistence, title, and playback effects
- **AND** the child does not receive a Service client, Player, persistence handle, credentials, or callbacks
