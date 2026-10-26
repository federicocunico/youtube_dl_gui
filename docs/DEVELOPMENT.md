# Development on Windows x64

Requirements: Windows 10/11 x64, Node 22.22.0, Rust 1.93.1 (selected by
`rust-toolchain.toml`), Visual Studio 2022 Build Tools with Desktop development
with C++, a Windows SDK, and WebView2. Windows `tar.exe` extracts pinned archives.
No global Python, FFmpeg or Deno is required.

With GNU Make installed, run `make all` to set up the project environment,
run deterministic tests, build the production installer, test its installed
runtime and collect the distributable under `artifacts/`. `make help` lists
individual targets. The same entry point works without Make:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build.ps1 -Task all
```

`make setup` checks the pinned Node version and Visual C++ compiler, installs
the pinned Rust toolchain through rustup, runs `npm ci`, provisions the private
runtime from hashes and installs the Playwright test browser. Install the
machine prerequisites listed above first; this command does not silently
replace a system Node installation or install Visual Studio. Stop a running
development server before setup so `npm ci` can replace its Windows binaries.

From the repository root in PowerShell:

```powershell
npm ci
npm run runtime
node scripts/check-runtime.mjs
npm run build
cargo build --manifest-path src-tauri/Cargo.toml --locked
resources/runtime/python/python.exe -I -m unittest discover -s tests -p "test_*.py" -v
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

For all deterministic gates and the Windows installer:

```powershell
npx playwright install chromium
npm run verify
npm run tauri -- build --ci --bundles nsis
node scripts/smoke-install.mjs "src-tauri/target/release/bundle/nsis/YT Saver_0.1.0_x64-setup.exe"
npm run verify:live
```

Do not run Cargo test suites concurrently: Windows locks executing test
binaries, so a second linker cannot replace the same executable. The installed
smoke uses private tools with an empty PATH and runs the Rust integration tests
against installed resources. Native WebView2 automation is CI-only.

The Windows workflow runs deterministic gates, builds and installs NSIS, checks
the production worker and native UI, and uploads verified artifacts. Publication
also requires successful live download/rollback validation of that exact
installer before a separate least-privilege job creates the tag and release.

## Automatic tags and releases

Set the same new SemVer in `package.json`, both root entries in
`package-lock.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`.
Regenerate and commit the Cargo lockfile after a version change with
`cargo check --manifest-path src-tauri/Cargo.toml`.
Commit the changes on `main`, sign in with `gh auth login` as the repository
owner, then run `make release`. The coordinator pushes `main`, monitors its
Windows workflow, downloads the exact CI installer and checks its hashes. It
installs that package and runs the real YouTube canaries and induced-regression
rollback locally. YouTube blocks GitHub-hosted data-center IPs, so these online
checks use the developer's network; they remain mandatory.

After success, the coordinator sends an authenticated `repository_dispatch`
request containing the source commit, successful build run ID, installer
SHA-256, timestamp and full test reports. The publication job accepts requests
only from the repository owner, verifies the successful main build, exact
installer, registered canaries, engine version and proof freshness, then pushes
the annotated version tag. It creates a draft release, uploads all assets and
publishes it. `LIVE-VALIDATION.json` and its checksum accompany the installer.

No additional token secret is required. Tag creation and release publication
share the same publication workflow because tags pushed with `GITHUB_TOKEN`
do not trigger another run. Existing tags are never moved. Ordinary pushes
produce CI artifacts; publication requires the verified local proof. A failed
or inconclusive online test stops the coordinator before dispatch. Re-running
a failed publication can resume its draft; published releases are preserved.

To work on the desktop UI:

```powershell
npm run tauri -- dev
```

The Tauri CLI starts Vite at port 1420. `npm run dev` alone opens a frontend
development server; privileged native actions require the desktop shell.
`npm run build` performs Svelte/TypeScript checks and generates `dist/`.

The first native build downloads and compiles dependencies and can take several
minutes. `--locked` prevents unexpected dependency resolution. Commit both npm
and Cargo lockfiles. Rust tests use the provisioned private interpreter; a
missing runtime is a test failure, not a skipped check.

## Private runtime

`resources/runtime-lock.json` pins upstream archive URLs and SHA-256 hashes.
`resources/engine-requirements.txt` pins every baseline Python wheel with its
hash. `npm run runtime` downloads, verifies, extracts and installs these pinned
components into the ignored `resources/runtime/` directory. It produces
`runtime/inventory.json` and includes upstream license texts. The integrity
check runs each binary by absolute path with an empty PATH.

Only maintainers updating the shipped baseline run:

```powershell
npm run runtime -- --resolve
```

Review the resulting lockfile, requirements and notices before committing.
FFmpeg pins a previous month-end upstream archive, retained longer than daily
builds. Refresh the shipped pins before upstream archive retention expires.
This command intentionally selects current upstream releases; normal builds
must not use `--resolve`. Network access to GitHub, PyPI and Python package
assets is needed for provisioning. Runtime archives are cached under
`.runtime-cache/`; generated runtimes and caches must not be committed.

The app's Python is independent of the Windows Python launcher and system
site-packages. Workers run with `-I -u`, do not load user Python paths, and
communicate only through the bounded protocol in `worker/PROTOCOL.md`. Native
tools are addressed by absolute paths. A Windows Job Object owns each worker
and all descendants, terminating them together on cancellation or supervisor
exit. Raw stderr is drained without being retained in application logs.

The generated icon comes from `resources/icon.svg`:

```powershell
npm run tauri -- icon resources/icon.svg --output src-tauri/icons
```
