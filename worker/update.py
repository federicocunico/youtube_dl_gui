"""Private updater worker. Same bounded protocol, separate entry point."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil
import ssl
import subprocess
import sys
import tempfile
import time
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parent))
import certifi
from packaging.version import Version, InvalidVersion
from protocol import WorkerError, serve
import media


def tls_context():
    context=ssl.create_default_context()
    context.load_verify_locations(cafile=certifi.where())
    return context


def get_json(url):
    with urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "YT-Saver-Updater/0.1"}), timeout=25, context=tls_context()) as response:
        data = response.read(16 * 1024 * 1024 + 1)
    if len(data) > 16 * 1024 * 1024: raise WorkerError("network", "Metadati aggiornamento troppo grandi.")
    return json.loads(data)


def latest_stable(metadata):
    versions = []
    for raw, files in metadata["releases"].items():
        try: version = Version(raw)
        except InvalidVersion: continue
        if not version.is_prerelease and not version.is_devrelease and files and any(not f.get("yanked") for f in files): versions.append((version, raw))
    if not versions: raise WorkerError("network", "Nessuna versione stabile disponibile.")
    return max(versions)[1]


def pip_run(python, arguments, cwd):
    # The executable is always from the bundled/copied private runtime.
    result = subprocess.run([str(python), "-I", "-m", "pip", "--isolated", "--disable-pip-version-check", *arguments],
                            cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=240, creationflags=0x08000000)
    if result.returncode:
        text = result.stdout.decode("utf-8", "replace")
        if "Requires-Python" in text or "requires a different Python" in text or "No matching distribution" in text:
            raise WorkerError("runtime", "Versione non installabile sul runtime incluso. Può essere necessario aggiornare YT Saver.")
        raise WorkerError("network", "Installazione pip non riuscita; la versione attiva è stata conservata.")


def prepare(payload, emit):
    version = str(Version(payload["version"]))
    stage = Path(payload["stage"]).resolve()
    expected_parent = Path(payload["staging_root"]).resolve()
    if stage.parent != expected_parent or stage.exists(): raise WorkerError("runtime", "Directory staging non valida o già presente.")
    baseline = Path(payload["baseline_python"]).resolve()
    if not baseline.is_absolute() or not (baseline / "python.exe").is_file(): raise WorkerError("runtime", "Python baseline mancante.")
    stage.mkdir(parents=True)
    emit.send("phase", {"state": "installing", "version": version})
    shutil.copytree(baseline, stage / "python")
    python = baseline / "python.exe"
    report = stage / "resolution.json"
    pip_run(python, ["install", "--dry-run", "--ignore-installed", "--only-binary=:all:", "--index-url", "https://pypi.org/simple", "--report", str(report), f"yt-dlp[default]=={version}", "packaging"], stage)
    resolved = json.loads(report.read_text("utf-8"))["install"]
    wheelhouse = stage / "wheelhouse"; wheelhouse.mkdir()
    requirements = []
    manifest = []
    for package in resolved:
        info = package["download_info"]
        url = urllib.parse.urlparse(info["url"])
        if url.scheme != "https" or url.hostname != "files.pythonhosted.org" or not url.path.endswith(".whl"):
            raise WorkerError("runtime", "Origine del pacchetto non consentita.")
        digest = info["archive_info"]["hashes"]["sha256"]
        filename = Path(urllib.parse.unquote(url.path)).name
        target = wheelhouse / filename
        hasher = hashlib.sha256()
        with urllib.request.urlopen(info["url"], timeout=30, context=tls_context()) as response, target.open("wb") as output:
            count = 0
            while block := response.read(65536):
                count += len(block)
                if count > 100 * 1024 * 1024: raise WorkerError("runtime", "Pacchetto oltre il limite di dimensione.")
                hasher.update(block); output.write(block)
        if hasher.hexdigest() != digest: raise WorkerError("runtime", "Hash del pacchetto non valido. Aggiornamento rifiutato.")
        name, exact = package["metadata"]["name"], package["metadata"]["version"]
        requirements.append(f"{name}=={exact} --hash=sha256:{digest}")
        manifest.append({"name": name, "version": exact, "sha256": digest})
    locked = stage / "requirements.txt"; locked.write_text("\n".join(requirements) + "\n", encoding="utf-8")
    pip_run(stage / "python/python.exe", ["install", "--no-index", "--find-links", str(wheelhouse), "--require-hashes", "--only-binary=:all:", "--no-deps", "--upgrade", "-r", str(locked)], stage)
    pip_run(stage / "python/python.exe", ["check"], stage)
    installed = subprocess.run([str(stage / 'python/python.exe'), '-I', '-c', 'import yt_dlp.version; print(yt_dlp.version.__version__)'],capture_output=True,timeout=20,creationflags=0x08000000)
    if installed.returncode or Version(installed.stdout.decode().strip()) != Version(version):
        raise WorkerError('runtime', 'La versione installata non corrisponde al candidato richiesto.')
    shutil.rmtree(wheelhouse)
    (stage / "manifest.json").write_text(json.dumps({"version": version, "packages": manifest, "validated": False}), encoding="utf-8")
    return {"version": version, "stage": str(stage)}


def suite(payload, emit):
    manifest = json.loads(Path(payload["canaries"]).read_text("utf-8"))
    root = Path(payload["test_root"]); root.mkdir(parents=True, exist_ok=True)
    deadline = time.monotonic() + min(120, manifest["timeout_seconds"])
    limit = min(25 * 1024 * 1024, manifest["byte_limit"])
    budget = media.ByteBudget(limit, deadline)
    errors = []
    selected_urls = [payload["control_url"]] if payload.get("control_url") else [v["url"] for v in manifest["videos"]]
    for url in selected_urls:
        if time.monotonic() >= deadline or budget.bytes > limit: break
        try:
            with tempfile.TemporaryDirectory(prefix="suite-", dir=root) as directory:
                work = Path(directory)
                base = {"url": url, "bin": payload["bin"], "options": {"video_quality": 360, "container": "auto", "fragments": 1}, "deadline": deadline}
                info = media.inspect(base, emit)
                if len(info["selection"]["ids"]) != 2: raise WorkerError("source", "Canary privo di tracce separate.")
                results = []
                for audio in (False, True):
                    folder = work / ("audio" if audio else "video"); folder.mkdir()
                    request = {**base, "directory": str(folder), "options": {**base["options"], "audio_only": audio}}
                    transferred = media.transfer(request, emit, budget)
                    budget.check()
                    verified = media.finalize({**request, **transferred}, emit)
                    results.append({"audio_only": audio, "probe": verified["probe"], "selection": verified["selection"]})
                budget.check()
                return {"status": "pass", "url": url, "bytes": budget.bytes, "checks": results, "tested_at": int(time.time())}
        except Exception as error:
            error = media.classify(error)
            errors.append({"url": url, "code": error.code, "message": str(error)})
    regression = next((e for e in errors if e["code"] in ("engine", "protocol", "format")), None)
    return {"status": "fail" if regression else "inconclusive", "control_url": regression["url"] if regression else None, "errors": errors}


def dispatch(request, emit):
    try:
        operation = request.payload.get("operation")
        if operation == "latest":
            latest = latest_stable(get_json("https://pypi.org/pypi/yt-dlp/json"))
            return {"version": latest, "newer": Version(latest) > Version(request.payload["current_version"])}
        if operation == "prepare": return prepare(request.payload, emit)
        if operation == "suite": return suite(request.payload, emit)
        raise WorkerError("protocol", "Operazione updater non valida.")
    except WorkerError: raise
    except Exception as error: raise media.classify(error) from None


if __name__ == "__main__":
    from yt_dlp.version import __version__
    raise SystemExit(serve(sys.stdin.buffer, sys.stdout.buffer, dispatch, __version__))
