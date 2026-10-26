# Spec Delta

## Purpose

Keep the local download engine current through automatic candidate testing while preserving a working version and recovering from regressions.

## ADDED Requirements

### Requirement: Automatic stable updates
The application SHALL check for the latest stable yt-dlp release on startup, at most once per 24 hours, and on manual request. Every newer stable version SHALL be eligible regardless of year or major/minor numbering. Release-note matching SHALL be informational and SHALL NOT gate updates.

#### Scenario: Year or version scheme change
- **WHEN** a newer stable release changes its year or version numbering scheme
- **THEN** the application attempts isolated installation and validation rather than requiring a code update solely because of the version number

#### Scenario: Offline update check
- **WHEN** package metadata cannot be fetched
- **THEN** the installed engine remains usable and the UI shows a non-blocking update-check failure

### Requirement: Isolated package installation
The application SHALL install the exact candidate version and its required compatible dependencies with private pip into a separate application-owned environment. It SHALL NOT modify system Python, the active environment or running jobs. Unsupported Python or native-runtime requirements SHALL fail the candidate without damaging the active version.

#### Scenario: Candidate requires a newer interpreter
- **WHEN** the candidate cannot run on the bundled Python runtime
- **THEN** the application retains the previous engine and reports that an app update is needed to supply the required runtime

### Requirement: Real download validation
Before activation, the application SHALL test metadata extraction, a real YouTube download with separate video/audio and muxing, and an audio-only download using short public test media. It SHALL verify nonempty outputs, expected audio/video tracks and positive duration. Validation SHALL use temporary files with bounded time, bytes and concurrency and clean them afterward.

#### Scenario: Candidate passes all checks
- **WHEN** real downloads and media verification succeed within the validation limits
- **THEN** the candidate becomes eligible for activation

#### Scenario: Fake success or broken muxing
- **WHEN** a worker reports success but output is empty or lacks an expected stream
- **THEN** validation fails and the candidate is not activated

#### Scenario: Test media or network unavailable
- **WHEN** validation cannot finish because test media is inaccessible or the network fails
- **THEN** the result is inconclusive, the previous engine remains active and a later validation can retry

### Requirement: Atomic activation and crash recovery
The application SHALL activate a validated candidate atomically for newly started jobs and retain the previous validated version. Existing jobs SHALL finish using their original version. Interrupted installation or activation SHALL recover to a complete usable environment on restart.

#### Scenario: Update while downloading
- **WHEN** a candidate is activated while old-version jobs are running
- **THEN** those jobs keep their original environment and new jobs use the candidate

#### Scenario: Crash during activation
- **WHEN** the app terminates during promotion of a candidate
- **THEN** the next startup selects a complete validated environment and discards or resumes incomplete staging safely

### Requirement: Automatic rollback after regression
If an activated candidate exhibits an engine regression confirmed by a failing control test that succeeds on the previous engine, the application SHALL automatically restore the previous engine for new jobs, quarantine the failing candidate and retry affected nonterminal jobs once with the restored engine. Completed outputs SHALL NOT be overwritten or redownloaded.

#### Scenario: Confirmed post-activation regression
- **WHEN** the active candidate fails its control test and the previous engine passes the same test
- **THEN** rollback occurs automatically, affected jobs receive at most one recovery retry and the UI identifies both versions and the failure

#### Scenario: Network failure on both versions
- **WHEN** both candidate and previous version fail the control test due to network or source unavailability
- **THEN** the app does not repeatedly switch versions and reports an inconclusive environment failure

### Requirement: Update visibility and bounded retries
The UI SHALL expose installed and candidate versions, update and validation progress, last successful test, rollback history and actionable errors. A rejected candidate SHALL NOT be reinstalled on every launch; a newer version or explicit user retry SHALL re-enable testing. Inconclusive tests SHALL retry with backoff without blocking user downloads.

#### Scenario: Quarantined version on restart
- **WHEN** the latest stable version is the same candidate previously rejected for a confirmed regression
- **THEN** the app keeps the working version and shows the rejected candidate without starting an update loop
