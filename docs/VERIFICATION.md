# Verification record

Local verification: 2026-10-26, Windows 11 Pro 10.0.26200, Intel Core i9-12900F.
Tool versions and bundled binary hashes are recorded in `runtime-lock.json`
and the generated runtime inventory. Repeatable commands are in DEVELOPMENT.md.

`make all` passed the complete setup, 62 deterministic tests, production build,
installed-package checks and artifact collection. `make package` with zlib
compression completed in 100.4 seconds on this machine and produced a 151.76 MiB
installer. `make smoke` subsequently verified installation, the private runtime
and removal of the installed executable by the silent uninstaller.

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

The NSIS installer was installed and uninstalled locally in a path with spaces
and Unicode. Its own worker passed video, audio and MP3 output with an empty
PATH, binary-origin/hash checks and deliberate removal of its FFmpeg. Rust
integration tests ran against those installed resources, including a mixed
bulk with concurrent filename collisions and an inaccessible source, audio
cancel/retry, durable settings and crash-after-rename recovery. The live
download and induced rollback suite also passed against the installed runtime.
The release-gate tests explicitly reject failed, inconclusive, duplicate or
absent test evidence.

Local native UI automation was denied by the computer-use approval layer.
The CI-only desktop harness runs on the isolated Windows runner and checks
the installed production WebView2, settings persistence and remote-preview
IPC/file isolation. These checks passed on Windows Server 2025 in
[run 40338985006](https://github.com/federicocunico/youtube_dl_gui/actions/runs/40338985006),
commit `bfe5b1ad33fd5a8cc1fc9d6785792544bef5773d`. The run produced the verified
installer, checksums, inventory and notices. Native screenshots cover three
rendering scales plus dark/light settings and queue navigation.

The runner uses an elevated process. WebView2 ignores environment browser flags
in that case, so the CI harness applies Microsoft's documented HKLM override
only for `Frame.exe`, then removes it in `finally`. This configuration belongs
to the isolated test runner. Its screenshots and workflow result are the
evidence for native UI acceptance; no local native startup is claimed by the
browser tests.
