# Install YT Saver on Windows

YT Saver targets Windows 10 22H2 or Windows 11, x64. Python, pip, FFmpeg and Deno
are included in the installer. No developer tools or global Python are needed.

For a tagged version, open the repository's GitHub Releases page and download
the `YT-Saver_*_x64-setup.exe` attachment. For a development build, open GitHub
Actions → Windows build → a successful main-branch run → Artifacts →
`YT-Saver-windows-x64`. GitHub requires sign-in to download workflow artifacts.
Extract the ZIP before running the installer. Builds are unsigned; Windows may
show an unknown-publisher/SmartScreen prompt. Only continue with a build whose
repository and checksum you have verified.

Compare the SHA-256 value in the accompanying `SHA256SUMS.txt`:

```powershell
Get-FileHash .\YT-Saver_0.1.0_x64-setup.exe -Algorithm SHA256
```

Run the setup and choose a folder if desired. Installation is per user. Paths
with spaces and Unicode are supported. The WebView2 bootstrapper installs the
Microsoft runtime if absent and may need internet access. Start YT Saver from the
Start menu, choose an existing writable download folder in Impostazioni, and
follow the user guide. Downloaded files remain in the folder you selected.

Uninstall through Windows Settings → Apps → Installed apps → YT Saver. The app's
queue, settings and validated engine environments live under the per-user
local app-data directory for `io.frame.downloader`, independently of the
installation folder. Uninstalling does not remove your downloaded media.

The distribution includes SHA-256 checksums, `runtime-lock.json`,
`engine-requirements.txt`, runtime inventory and third-party notices. Upstream
license texts are installed under `runtime/licenses`. The baseline is pinned;
runtime engine updates use hash-verified wheels from PyPI and do not update
your system Python. A new native app release is required when a dependency
needs a newer Python or a bundled native tool.

Only successful verified builds produce downloadable installers. Tag releases
also require live YouTube downloads and the induced-regression rollback test;
a failure or inconclusive result prevents publication. Artifacts expire after
30 days; release attachments remain associated with their version.
