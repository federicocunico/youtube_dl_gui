# Spec Delta

## Purpose

Provide a clear and responsive local desktop interface for identifying media, managing downloads and understanding operational failures.

## ADDED Requirements

### Requirement: Coherent desktop workflow
The application SHALL provide Italian-language views for adding media, inspecting and configuring selections, managing the queue and changing settings. It SHALL use consistent typography, spacing and visual hierarchy, light/dark themes and layouts usable at a 1024 by 700 logical-pixel window without hiding primary actions.

#### Scenario: Add a video to the queue
- **WHEN** a user pastes a video link in the add-media view
- **THEN** the user can inspect its preview, choose output options and enqueue it within the same workflow

### Requirement: Accessible and explicit states
Primary controls SHALL support keyboard navigation, visible focus, accessible labels and text explanations independent of color. Loading, empty, offline, error, updating and completed states SHALL offer clear next actions and avoid modal interruption for background update failures.

#### Scenario: Keyboard-only audio download
- **WHEN** a keyboard-only user submits a link, enables audio-only, chooses a folder and enqueues
- **THEN** every required control is reachable and its state is conveyed by labels and focus

### Requirement: Local data and controlled privileges
The application SHALL keep queue data, preferences and download history locally and require no app account or cloud backend. Remote media content SHALL NOT access local files or privileged app commands. Diagnostics SHALL omit authentication data and signed media URLs.

#### Scenario: Remote preview content
- **WHEN** an embedded player loads third-party content
- **THEN** it cannot invoke filesystem, process or download-management commands

### Requirement: File actions and close behavior
Completed items SHALL provide native actions to open the final file and reveal its folder. Closing the app during active downloads SHALL offer cancel-and-close or keep-running choices; confirmed closure SHALL preserve recoverable queue state.

#### Scenario: Close with active jobs
- **WHEN** the user confirms cancel-and-close while downloads are active
- **THEN** workers are stopped, partial data is retained for supported resume and interrupted items restore as paused

#### Scenario: Open a missing completed file
- **WHEN** the user opens an output removed outside the app
- **THEN** the UI reports the missing file without deleting history or crashing
