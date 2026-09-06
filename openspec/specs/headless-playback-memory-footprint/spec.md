# headless-playback-memory-footprint Specification

## Purpose

Bounds the resident memory a headless Player owner accumulates over a long
playback session, so a small always-on host can serve audio indefinitely
without exhausting RAM and swap.

## Requirements

### Requirement: Headless playback resident memory SHALL NOT grow without bound across track transitions

A Player owner running headless SHALL reach a steady resident-memory plateau
during continuous playback. Resident memory measured after a long sequence of
ordinary track transitions SHALL NOT exceed the plateau observed early in the
same session by a margin that scales with the number of transitions performed.

Freed playback memory SHALL be returned to the operating system during the
session, not only at process exit.

#### Scenario: Ordinary listening session does not exhaust host memory

- **WHEN** a headless Player owner plays audio continuously through an ordinary
  listening session on a host with 1 GiB of RAM
- **THEN** resident memory SHALL plateau rather than rise with each transition,
  and the host SHALL NOT exhaust RAM or swap

#### Scenario: Memory is reclaimed without stopping playback

- **WHEN** a headless playback session has been running long enough for many
  track transitions to have completed
- **THEN** resident memory SHALL have fallen back toward the session plateau
  without requiring playback to be stopped or the process to exit

### Requirement: Headless playback SHALL reserve an audio-sized playback cache

A headless Player owner SHALL configure a playback cache budget sized for audio
sources. It SHALL NOT reserve the retained-cache budget provisioned for video
playback.

#### Scenario: Headless audio run

- **WHEN** a Player owner starts a headless playback run
- **THEN** its forward and retained playback cache budgets SHALL together be a
  small fraction of the video retained-cache budget, and playback SHALL start
  and continue normally

#### Scenario: Cache sizing does not change source resolution

- **WHEN** a headless run plays an Emby item, an Audiobookshelf item, or a feed
  entry
- **THEN** the reduced cache budget SHALL NOT change the resolved source URL,
  the selected format, or queue ordering for any item kind

### Requirement: Track transitions SHALL NOT create per-transition background threads

Playback progress and session reporting for a run SHALL be performed by
long-lived workers. A track transition SHALL NOT be the trigger for creating a
new operating-system thread.

#### Scenario: Reporting order across a transition

- **WHEN** playback transitions from one queue item to the next
- **THEN** the outgoing item's stopped report SHALL be submitted before the
  incoming item's start report, and playback SHALL NOT wait on either

#### Scenario: Thread count is stable across transitions

- **WHEN** a headless run performs many ordinary track transitions
- **THEN** the process thread count attributable to session reporting SHALL NOT
  grow with the number of transitions

#### Scenario: Reporting failure does not stall playback

- **WHEN** a stopped or start report cannot be delivered to the server
- **THEN** the failure SHALL be logged and subsequent transitions SHALL continue
  to be reported without the run stalling
