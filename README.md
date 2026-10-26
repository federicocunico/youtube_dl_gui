# Frame

Desktop YouTube downloader for Windows 10/11 x64, built with Svelte, Tauri and a
private Python engine. Download video with audio, extract or convert audio,
inspect videos and playlists, and manage a persistent parallel download queue.
The engine updates in isolation and is activated only after real test downloads;
a confirmed regression automatically restores the previous engine.

Get the precompiled installer from [GitHub Actions](https://github.com/federicocunico/youtube_dl_gui/actions/workflows/windows.yml)
or versioned [releases](https://github.com/federicocunico/youtube_dl_gui/releases).
See the [installation guide](docs/INSTALLATION.md) and [user guide](docs/USER-GUIDE.md).

See [development instructions](docs/DEVELOPMENT.md) for a local build and the
[worker protocol](worker/PROTOCOL.md) for the native/engine boundary. No global
Python installation is required. Runtime source and licenses are recorded in
[third-party notices](resources/THIRD-PARTY-NOTICES.md).
