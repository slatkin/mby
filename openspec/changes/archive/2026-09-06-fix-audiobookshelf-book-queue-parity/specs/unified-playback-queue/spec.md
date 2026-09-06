## ADDED Requirements

### Requirement: Service-specific refresh preserves unrelated queue-item kinds
A refresh sourced from one Service SHALL update or prune only queue slots that belong to that Service. It SHALL preserve every slot belonging to another Service or to Feeds, including both Audiobookshelf podcast episodes and Audiobookshelf books, without attempting to resolve their identities through the refreshing Service.

#### Scenario: Emby refresh observes an Audiobookshelf book
- **WHEN** an Emby refresh merges into a queue containing an inactive Audiobookshelf book
- **THEN** the book slot SHALL remain in the same canonical position
- **AND** SHALL NOT be reported as pruned because it has no Emby identity

#### Scenario: Emby refresh observes both Audiobookshelf shapes
- **WHEN** an Emby refresh merges into a mixed queue containing a podcast episode and a book from Audiobookshelf
- **THEN** both Audiobookshelf slots SHALL remain unchanged
- **AND** Emby-owned slots SHALL continue to reconcile normally
