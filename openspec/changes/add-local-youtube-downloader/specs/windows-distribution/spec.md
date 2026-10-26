# Spec Delta

## Purpose

Deliver a prebuilt Windows application through GitHub with included media dependencies and repeatable release verification.

## ADDED Requirements

### Requirement: Self-contained Windows installer
The project SHALL produce a per-user Windows 10/11 x64 setup executable containing the application, private Python with pip, a validated baseline engine, FFmpeg/ffprobe and its required JavaScript runtime and solver components. Users SHALL NOT need system Python, Node, Rust or FFmpeg. Missing WebView2 SHALL be handled by the installer with a disclosed network requirement.

#### Scenario: Clean Windows installation
- **WHEN** a user installs on a supported Windows system without developer or media tools
- **THEN** the app starts and can download and mux media using only its packaged tools after any required WebView2 bootstrap completes

### Requirement: Continuous Windows build artifacts
GitHub Actions SHALL run verification and build the Windows installer on pull requests, pushes to main and manual dispatch. Successful trusted builds SHALL publish a downloadable installer artifact, dependency version inventory, SHA-256 checksums and third-party notices. Pull request builds SHALL have no release-write privileges or signing secrets.

#### Scenario: Main build
- **WHEN** a commit is pushed to main
- **THEN** the workflow verifies the application, builds its installer and uploads the successful artifacts

#### Scenario: Verification failure
- **WHEN** a required test or packaged-runtime check fails
- **THEN** the workflow fails and does not publish a release

### Requirement: Versioned GitHub releases
A valid version tag matching the app version SHALL trigger creation of a GitHub Release containing the verified installer, checksums and dependency notices. Publication SHALL occur only after all required verification and packaging jobs succeed. Signature status SHALL be disclosed; signing SHALL NOT be claimed without configured credentials.

#### Scenario: Versioned release
- **WHEN** a matching version tag passes the release workflow
- **THEN** the release offers the precompiled installer and its checksum without requiring users to compile the source

### Requirement: Packaged-runtime verification
CI SHALL verify the installed package on Windows with system developer tools excluded from the app's runtime lookup. It SHALL exercise startup, the private pip environment, engine protocol, a local media download and mux, audio-only output and rollback. Release workflows SHALL also require successful live YouTube canary validation.

#### Scenario: Hidden reliance on system FFmpeg
- **WHEN** the packaged FFmpeg is missing but a runner has FFmpeg on PATH
- **THEN** packaged-runtime verification fails rather than passing through the runner's installation

#### Scenario: Live validation unavailable at release time
- **WHEN** release canary downloads fail or remain inconclusive
- **THEN** release publication is withheld with diagnostics and can be retried after the external failure is resolved

### Requirement: Observed workflow completion
After any push made as part of delivering this change that triggers CI, the delivery process SHALL monitor the corresponding commit's workflow to a terminal state, diagnose and repair failures when possible and report the final result.

#### Scenario: Failing workflow after delivery push
- **WHEN** a delivery push triggers a failing build
- **THEN** the failure is investigated, a fix is verified and pushed, and its workflow is monitored until green or an explicit unresolved blocker is reported
