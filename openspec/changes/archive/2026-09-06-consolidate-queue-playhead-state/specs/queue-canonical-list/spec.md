## MODIFIED Requirements

### Requirement: Queue projection is bounded presentation data

Queue SHALL project selectable rows with stable opaque `QueueSlotId` targets and presentation metadata, semantic active state, and optional integer `progress_percent` clamped to `0..=100`. The projection SHALL NOT carry ticks, runtime, source preparation, credentials, callbacks, or provider effects.

The projection's semantic active state — which queue scope is playing, which slot within it, and whether that slot is confirmed by the playback owner or is an optimistic prediction awaiting confirmation — SHALL be one owned value, reconciled against playback-owner status in one place outside the render path. An optimistic prediction SHALL record why it is optimistic: a queue edit relocated the still-playing item, or a different item was selected to play. Reconciliation SHALL clear a prediction once the owner's reported slot and queue length match it.

While a prediction says a different item was selected, the projection SHALL report that item's slot as active with no `progress_percent` until reconciliation confirms it. While a prediction says the still-playing item was relocated, and whenever the active slot is confirmed, `progress_percent` SHALL follow the live playback position.

A push that forces the child to adopt a specific active index SHALL be scoped to one queue scope and SHALL be consumed only while that scope is the one on screen; a push armed for a scope the user is not viewing SHALL be dropped rather than move the viewed scope's independent selection.

#### Scenario: Active progress is safe to paint

- **WHEN** an active Queue slot has progress outside the presentation range
- **THEN** Queue clamps the projected percentage to `0..=100`
- **AND** the child paints only the bounded presentation value

#### Scenario: Refresh preserves target identity

- **WHEN** Queue content is refreshed without a navigation event
- **THEN** the child preserves its local cursor and scroll where the selected `QueueSlotId` remains present
- **AND** it clamps or resets only when the target is absent or content no longer permits the position
- **AND** the shell does not mirror the child cursor or scroll per frame

#### Scenario: Selecting a different item to play

- **WHEN** the user starts a different queue item and the playback owner has not yet reported the change
- **THEN** the projection reports the newly selected slot as active
- **AND** it reports no progress for that slot until the owner confirms the change
- **AND** it never carries the previously playing item's position or runtime onto the new slot

#### Scenario: A queue edit relocates the playing item

- **WHEN** a queue edit moves or removes rows such that the still-playing item's index changes, before the playback owner reports the new index
- **THEN** the projection reports the item's new index as active
- **AND** it keeps that item's existing progress unchanged

#### Scenario: A push targets a scope the user is not viewing

- **WHEN** an authoritative active-index push is armed for one queue scope while the user is viewing a different scope
- **THEN** the viewed scope's selection is unchanged
- **AND** the push does not take effect when the user later switches to its scope unless it is re-armed

#### Scenario: Reconciliation does not run during paint

- **WHEN** the queue projection or playback indicator is read to render a frame
- **THEN** reading it does not consume or clear any pending prediction
- **AND** predictions are cleared only by the single reconciliation step that runs against playback-owner status
