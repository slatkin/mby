## ADDED Requirements

### Requirement: Audiobookshelf teardown purges every Audiobookshelf queue-item shape
Audiobookshelf Service replacement or removal SHALL finalize an active Audiobookshelf lifecycle and remove every Audiobookshelf-owned slot from the affected owner's live and persisted queue state. This SHALL include both podcast episodes and books while preserving Emby items and Feed entries.

#### Scenario: Service removal with mixed Audiobookshelf queue
- **WHEN** Audiobookshelf is removed while the queue contains podcast episodes, books, Emby items, and Feed entries
- **THEN** every Audiobookshelf episode and book slot SHALL be removed from live and persisted queue state
- **AND** every Emby item and Feed entry SHALL remain in canonical order

#### Scenario: Service replacement while a book is active
- **WHEN** Audiobookshelf is replaced with a different server while a book playback session is active
- **THEN** the prior book session SHALL be finalized before its slot and owner context are discarded
- **AND** the book SHALL NOT survive for resolution against the replacement server

#### Scenario: Interactive process removes Audiobookshelf
- **WHEN** the interactive process removes Audiobookshelf from either a Composed queue or its in-process owner's Bound queue
- **THEN** both Audiobookshelf queue-item shapes SHALL be purged through the same Service-owned-state rule
