# Spec Delta

## Purpose

Let users control the downloaded media, its quality and final destination while producing valid files with predictable names.

## ADDED Requirements

### Requirement: Video with audio output
The application SHALL default to video with audio and produce one final muxed file. It SHALL select compatible available streams for the requested quality and container, disclose codec limitations and avoid implicit video transcoding.

#### Scenario: Separate video and audio streams
- **WHEN** the chosen quality is available only as separate streams
- **THEN** the application downloads both and muxes them into one playable file before declaring completion

#### Scenario: Unsupported container combination
- **WHEN** available codecs cannot be muxed into the requested container
- **THEN** the UI offers a compatible container or quality before starting rather than silently changing the request

### Requirement: Quality selection
The application SHALL offer best available and explicit video resolution limits, and best source audio or supported audio format and bitrate presets. It SHALL show actual selected resolution, codecs and audio quality and label unavailable choices.

#### Scenario: Resolution cap
- **WHEN** a user selects up to 1080p and the source has 720p and 2160p
- **THEN** the application selects 720p and displays the actual selection without upscaling

#### Scenario: Audio bitrate preset
- **WHEN** a user requests MP3 at a supported bitrate
- **THEN** the UI identifies conversion and the output uses that bitrate without claiming to improve source quality

### Requirement: Audio only mode
The application SHALL provide an audio-only toggle that downloads no video stream and outputs either original audio or the selected supported audio conversion. Video controls SHALL be disabled in audio-only mode.

#### Scenario: Original audio extraction
- **WHEN** the user enables audio-only and selects original audio
- **THEN** the application saves the selected source audio without lossy re-encoding

### Requirement: Destination and filename safety
The application SHALL allow native folder selection and per-item filename editing. It SHALL sanitize Windows-invalid names, confine writes to the selected directory, check writability and resolve collisions with a unique suffix by default without overwriting existing files.

#### Scenario: Invalid filename and collision
- **WHEN** a proposed title contains reserved characters and the sanitized filename already exists
- **THEN** the application previews a valid unique filename and preserves the existing file

#### Scenario: Destination becomes unwritable
- **WHEN** a target directory disappears, lacks permission or runs out of disk space
- **THEN** the item fails with an actionable destination error and other downloads continue

### Requirement: Persistent defaults and job snapshots
The application SHALL persist global destination, mode, quality and container defaults and allow per-item overrides. Each enqueued job SHALL retain its own settings snapshot; changing defaults SHALL affect only subsequent jobs.

#### Scenario: Defaults changed during downloads
- **WHEN** a user changes default quality and folder after enqueueing a batch
- **THEN** existing jobs retain their configured output settings and new jobs use the changed defaults
