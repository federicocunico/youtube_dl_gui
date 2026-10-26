# Spec Delta

## Purpose

Manage durable batches of downloads with bounded parallelism, visible progress and recovery from interruption or item failures.

## ADDED Requirements

### Requirement: Bounded scheduling
The application SHALL provide a persistent ordered queue with configurable simultaneous jobs and per-job fragment parallelism. Defaults SHALL be three active jobs and four fragments per job; accepted ranges SHALL be one to eight for each. Metadata work and muxing SHALL have separate bounded resource limits.

#### Scenario: Concurrency limit
- **WHEN** ten jobs are queued with a simultaneous-job limit of three
- **THEN** at most three jobs are active and waiting jobs start as slots become free

#### Scenario: Reduced limit
- **WHEN** the user reduces concurrency below the current active count
- **THEN** running jobs continue and no new job starts until the active count is below the new limit

### Requirement: Observable job lifecycle
The application SHALL distinguish queued, resolving, downloading, paused, muxing, completed, failed and cancelled states. It SHALL show per-item and aggregate byte progress, speed and ETA when known, mark unknown totals explicitly and keep muxing visible until output verification succeeds.

#### Scenario: Download finished but muxing active
- **WHEN** the streams finish transferring and post-processing begins
- **THEN** the job shows muxing rather than completed until its final file is verified

### Requirement: Queue controls
The application SHALL support enqueueing selected items, reordering waiting jobs, pause/resume, cancellation, retry of failed items and removal of finished items. Pausing SHALL release active worker slots and preserve resumable temporary files when supported.

#### Scenario: Cancel during muxing
- **WHEN** a user cancels a job during post-processing
- **THEN** its process tree stops, no incomplete file is presented as completed and unrelated jobs continue

#### Scenario: Resume without server support
- **WHEN** a paused item resumes but the source cannot continue partial transfers
- **THEN** the job restarts the required transfer and explains the restart

### Requirement: Durable recovery
The application SHALL persist queue order, job settings, temporary-file references and terminal outcomes. After unexpected termination it SHALL restore interrupted jobs as paused, reconcile completed files and avoid automatic duplicate execution.

#### Scenario: Restart after a crash
- **WHEN** the application restarts after a crash during two transfers
- **THEN** both jobs appear paused with their settings intact and can resume without duplicating completed outputs

### Requirement: Independent failures and bounded retries
The application SHALL isolate per-item failures and retry transient network or rate-limit errors at most three times with backoff, respecting server retry delays. Permanent failures SHALL remain inspectable and manually retryable.

#### Scenario: Rate limit in one item
- **WHEN** one job receives a retryable rate-limit response
- **THEN** it waits according to the retry policy while unrelated jobs remain operable

### Requirement: Responsiveness during bulk work
The application SHALL keep queue controls usable during discovery, transfers and post-processing. On the documented Windows reference machine, a 100-item fixture queue with three simulated active transfers SHALL acknowledge pause and cancel actions within 500 ms, excluding worker shutdown time.

#### Scenario: Bulk queue control latency
- **WHEN** the responsiveness acceptance scenario runs
- **THEN** UI acknowledgement meets the target and metadata or progress events do not block interaction
