## MODIFIED Requirements

### Requirement: Network video feed playback uses a 100MiB retained cache window

When a Player owner plays a network-backed video feed entry with a video window,
the playback cache SHALL be configured to retain up to 100MiB of previously
demuxed data. The existing 50MiB forward buffering limit SHALL remain unchanged
for those runs. Headless runs SHALL instead use the audio-sized cache budget
defined by `headless-playback-memory-footprint`.

#### Scenario: High-quality video feed tolerates a short throughput dip

- **WHEN** a video feed entry is played at a high bitrate with a video window and
  the source throughput temporarily falls below the playback bitrate
- **THEN** playback SHALL have the 100MiB retained cache budget available before
  entering repeated buffering

#### Scenario: Normal video feed playback starts

- **WHEN** a video feed entry is loaded through the normal feed play path with a
  video window
- **THEN** the player SHALL use the configured retained-cache policy without
  changing the feed's resolved source URL or selected format

#### Scenario: Headless run does not reserve the video retained cache

- **WHEN** a playback run is headless
- **THEN** it SHALL NOT reserve the 100MiB retained cache window, and its source
  resolution and format selection SHALL be unchanged
