# Spec Delta

## Purpose

Allow users to identify YouTube media before downloading and select individual items from bulk input and playlists.

## ADDED Requirements

### Requirement: Supported YouTube inputs
The application SHALL accept HTTPS YouTube watch, Shorts, youtu.be and playlist URLs. It SHALL reject unsupported hosts, malformed URLs, channel roots and site homepages with an actionable message before scheduling downloads.

#### Scenario: Short link
- **WHEN** a user submits a valid youtu.be link
- **THEN** the application resolves its video identity and starts metadata discovery

#### Scenario: Homepage or deceptive hostname
- **WHEN** a user submits youtube.com without a media identifier or youtube.com.example.org
- **THEN** the application requests a video or playlist URL and creates no download

### Requirement: Media identification preview
The application SHALL display title, channel, thumbnail, duration when known, available formats and proposed output filename before enqueueing a discovered item. Unavailable metadata SHALL have explicit placeholders rather than block otherwise valid media.

#### Scenario: Successful discovery
- **WHEN** metadata discovery succeeds
- **THEN** the preview shows the media identity and selectable formats without downloading the full media

#### Scenario: Unavailable video
- **WHEN** the video is removed, private, requires login or is an active live stream
- **THEN** discovery displays the specific unsupported or unavailable state without stalling other items

### Requirement: Bulk and playlist selection
The application SHALL accept multiple newline-separated URLs and enumerate playlists with incremental results, cancellation and selectable items. It SHALL flag duplicate video identities within an import and require explicit selection to enqueue them again.

#### Scenario: Mixed bulk input
- **WHEN** a user imports valid links, invalid links and duplicate videos
- **THEN** valid distinct items remain selectable and invalid or duplicate items show individual explanations

#### Scenario: Playlist with unavailable entries
- **WHEN** playlist discovery encounters inaccessible entries
- **THEN** accessible entries remain selectable and the inaccessible entries are marked individually

#### Scenario: Cancelling a large playlist
- **WHEN** the user cancels enumeration after partial results
- **THEN** discovery stops and the already discovered items remain available for selection

### Requirement: Optional playback preview
The application SHALL provide a user-initiated playback preview and a browser fallback. Remote playback SHALL have no access to privileged desktop commands, SHALL NOT autoplay and SHALL NOT be required for downloads.

#### Scenario: Embedding denied
- **WHEN** embedding is blocked or playback is unavailable
- **THEN** the thumbnail and metadata remain visible, the browser action is available and enqueueing still works
