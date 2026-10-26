# Verification record

Local verification: 2026-10-26, Windows 11 Pro 10.0.26200, Intel Core i9-12900F.
Tool versions and bundled binary hashes are recorded in `runtime-lock.json`
and the generated runtime inventory. Repeatable commands are in DEVELOPMENT.md.

The browser UI suite exercises bulk discovery and duplicate selection, a
keyboard-only audio workflow, missing output files, update errors, close
confirmation and primary-action visibility at 1024×700 with pixel scales
1, 1.5 and 2. These browser scales measure rendering; native WebView2 screenshots
are collected separately by the Windows CI installed-app smoke test.

The queue fixture renders 100 jobs with three active simulated transfers and
progress updates every 200 ms. The 2026-10-26 run measured pause acknowledgement
at 0.30 ms and cancel at 0.10 ms, against a 500 ms bound. This measures the UI
dispatch/ack path with a controlled native adapter, not network cancellation
latency. Rust tests separately verify actual worker and FFmpeg descendant
termination, slot release, retry/restart persistence and crash recovery.
`test-results/ui-report.json` contains timings; screenshots and machine-specific
results are transient CI artifacts rather than source files.

Both registered live YouTube canaries passed video+audio mux and audio-only
checks on 2026-10-26. ffprobe reported 5.021 seconds, VP9 video and Opus audio;
combined transfers were 11,647 and 12,279 bytes. The real update integration
also installed the pinned stable engine in a separate private Python, passed
its downloads, promoted it while a baseline job was active, induced an engine
regression, confirmed the previous version still passed and rolled back with
one recovery retry. `npm run verify:live` reproduces these checks. Availability
of a YouTube source depends on region, network and upstream service behavior;
these results demonstrate the tested sources and date.

The deterministic suite uses local HTTP/DASH media to verify real separate
downloads, mux, original audio, MP3/AAC conversion, track/duration validation,
safe filenames, atomic non-overwrite publication and recovery after rename.
Updater tests cover stable-version selection, hash failure, incompatible
Python requirements, differential verdicts, quarantine, retry schedule,
promotion, persistence and environment retention.

Local native UI automation was denied by the computer-use approval layer.
The CI-only desktop harness runs on the isolated Windows runner and checks
the installed production WebView2, settings persistence and remote-preview
IPC isolation. Its screenshots and final workflow result are the evidence for
native UI acceptance; no local native startup is claimed by the browser tests.
