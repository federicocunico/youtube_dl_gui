"""YouTube adapter. Transfer and post-processing are separate scheduler phases."""
from __future__ import annotations

import json
import math
from pathlib import Path
import re
import subprocess
import time
from urllib.error import URLError
from email.utils import parsedate_to_datetime

import yt_dlp
import yt_dlp.globals
from protocol import WorkerError, redact


class QuietLogger:
    def __init__(self, callback=None): self.callback = callback
    def debug(self, message):
        if self.callback: self.callback(str(message))
    def info(self, message): pass
    def warning(self, message):
        if self.callback: self.callback(str(message))
    def error(self, message): pass


class ResumeStatus:
    def __init__(self, previous_size=0, fragment_index=0):
        self.previous_size = previous_size
        self.fragment_index = fragment_index
        self.resuming = False
        self.restarting = False

    def message(self, text):
        if 'Resuming download at byte' in text: self.resuming = True
        if 'Unable to resume' in text or 'Restarting from the beginning' in text: self.restarting = True

    def detail(self, progress):
        if not self.previous_size: return 'Scaricamento'
        if self.restarting: return 'La sorgente ha riavviato il trasferimento'
        if self.resuming or self.fragment_index > 0 and (progress.get('fragment_index') or 0) >= self.fragment_index:
            return 'Ripresa dai dati parziali'
        return 'Verifica della ripresa dei dati parziali'


class ByteBudget:
    def __init__(self, limit, deadline):
        self.limit, self.deadline = limit, deadline
        self.bytes = 0
        self.streams = {}

    def check(self):
        if self.bytes > self.limit: raise WorkerError('network', 'Budget byte della validazione superato.')
        if time.monotonic() >= self.deadline: raise WorkerError('network', 'Tempo della validazione superato.')

    def observe(self, stream, current):
        previous = self.streams.get(stream, 0)
        self.bytes += current - previous if current >= previous else current
        self.streams[stream] = current
        self.check()


def classify(error: Exception) -> WorkerError:
    if isinstance(error, WorkerError): return error
    text = str(error)
    lower = text.lower()
    if any(word in lower for word in ("private video", "unavailable", "removed", "sign in", "not available", "copyright", "members-only", "404", "http error 403")):
        return WorkerError("source", "Video non disponibile o richiede accesso: " + redact(text))
    if isinstance(error,(URLError,ConnectionError,TimeoutError)) or any(word in lower for word in ("timed out", "timeout", "429", "too many requests", "connection", "network", "temporarily", "http error 5", "getaddrinfo")):
        delay = None
        current = error
        for _ in range(5):
            headers = getattr(current, "headers", None) or getattr(getattr(current, "response", None), "headers", None)
            raw = headers.get("Retry-After") if headers else None
            if raw:
                try: delay = max(0, int(raw))
                except (ValueError, TypeError):
                    try: delay = max(0, int(parsedate_to_datetime(raw).timestamp() - time.time()))
                    except (ValueError, TypeError, OverflowError): pass
                break
            nested = getattr(current, "exc_info", None)
            current = (nested[1] if nested else None) or getattr(current, "cause", None) or getattr(current, "__cause__", None)
            if current is None: break
        return WorkerError("network", "Errore temporaneo di rete: " + redact(text), True, delay)
    if isinstance(error, OSError):
        return WorkerError("destination", "Impossibile scrivere nella destinazione: " + redact(text))
    if "requested format" in lower:
        return WorkerError("format", "La qualità richiesta non è disponibile. Analizza nuovamente il video.")
    return WorkerError("engine", "Errore del motore: " + redact(text))


def options(payload):
    binary = Path(payload["bin"]).resolve()
    for name in ("ffmpeg.exe", "ffprobe.exe", "deno.exe"):
        if not (binary / name).is_file(): raise WorkerError("runtime", f"Componente incluso mancante: {name}")
    yt_dlp.globals.plugin_dirs.value = []
    return dict(quiet=True, no_warnings=True, logger=QuietLogger(), noplaylist=True,
                cachedir=False, socket_timeout=20, retries=0, fragment_retries=0,
                extractor_retries=0, ffmpeg_location=str(binary),
                js_runtimes={"deno": {"path": str(binary / "deno.exe")}},
                remote_components=[], fixup="never", windowsfilenames=True,
                concurrent_fragment_downloads=max(1, min(8, int(payload.get("options", {}).get("fragments", 4)))))


def finite(value):
    return value if isinstance(value, (int, float)) and math.isfinite(value) else None


def normalized(info):
    identifier = str(info.get("id") or "")
    thumbnail = info.get("thumbnail")
    if not isinstance(thumbnail, str) or not thumbnail.startswith("https://"): thumbnail = None
    return dict(id=identifier, url=info.get("webpage_url") or f"https://www.youtube.com/watch?v={identifier}",
                title=info.get("title") or "Titolo non disponibile", channel=info.get("channel") or info.get("uploader") or "Canale non disponibile",
                thumbnail=thumbnail, duration=finite(info.get("duration")),
                filename=info.get("title") or identifier,
                live=bool(info.get("is_live") or info.get("live_status") in ("is_live", "is_upcoming")),
                formats=[{k: f.get(k) for k in ("format_id", "ext", "height", "width", "fps", "vcodec", "acodec", "abr", "tbr", "filesize", "filesize_approx")}
                         for f in info.get("formats", []) if not f.get("has_drm") and f.get("ext") != "mhtml"])


def inspect(payload, emit):
    with yt_dlp.YoutubeDL(options(payload)) as ydl:
        info = ydl.extract_info(payload["url"], download=False)
    result = normalized(info)
    if result["live"]: raise WorkerError("source", "Le dirette attive o programmate non sono supportate.")
    if not result["formats"]: raise WorkerError("source", "Nessun formato scaricabile disponibile.")
    result["selection"] = choose_formats(info, payload.get("options", {}))
    return result


def enumerate_playlist(payload, emit):
    opts = options(payload)
    opts.update(noplaylist=False, extract_flat="in_playlist", lazy_playlist=True, ignoreerrors=True, skip_download=True)
    count = 0
    with yt_dlp.YoutubeDL(opts) as ydl:
        info = ydl.extract_info(payload["url"], download=False)
        if not info: raise WorkerError("source", "Playlist non disponibile.")
        for index, entry in enumerate(info.get("entries", [])):
            if entry is None:
                emit.send("item", {"index": index, "error": "Elemento non disponibile.", "available": False})
            else:
                item = normalized(entry)
                item.update(index=index, available=bool(re.fullmatch(r"[A-Za-z0-9_-]{11}", item["id"])))
                emit.send("item", item)
            count += 1
    return {"count": count, "title": info.get("title")}


def _video_ok(format, container):
    codec = str(format.get("vcodec") or "none")
    if codec == "none": return False
    if container == "webm": return codec.startswith(("vp8", "vp9", "vp09", "av01"))
    if container == "mp4": return codec.startswith(("avc", "h264", "hev", "hvc", "hevc", "av01", "mpeg4"))
    return True


def _audio_ok(format, container):
    codec = str(format.get("acodec") or "none")
    if codec == "none": return False
    if container == "webm": return codec.startswith(("opus", "vorbis"))
    if container == "mp4": return codec.startswith(("mp4a", "aac", "mp3", "alac"))
    return True


def choose_formats(info, opts):
    formats = [f for f in info.get("formats", []) if not f.get("has_drm") and f.get("ext") != "mhtml"]
    audio = [f for f in formats if f.get("vcodec", "none") == "none" and f.get("acodec", "none") != "none"]
    # extract_info returns formats ordered by yt-dlp's codec/source preferences.
    audio.reverse()
    if opts.get("audio_only"):
        if not audio: raise WorkerError("format", "Non è disponibile una traccia solo audio.")
        selected = audio[0]
        extension = selected.get("ext", "m4a") if opts.get("audio_format", "original") == "original" else opts["audio_format"]
        return {"ids": [selected["format_id"]], "container": extension, "height": None, "vcodec": "none", "acodec": selected.get("acodec"), "abr": selected.get("abr"), "estimate": selected.get("filesize") or selected.get("filesize_approx")}
    container = opts.get("container", "auto")
    cap = int(opts.get("video_quality") or 0)
    video = [f for f in formats if _video_ok(f, container) and (not cap or (f.get("height") or 0) <= cap)]
    rank = {id(f): index for index, f in enumerate(formats)}
    video.sort(key=lambda f: (f.get("height") or 0, rank[id(f)]), reverse=True)
    for selected in video:
        if selected.get("acodec", "none") != "none":
            if not _audio_ok(selected, container): continue
            tracks = [selected]
            audio_selected = selected
        else:
            compatible = [f for f in audio if _audio_ok(f, container)]
            if not compatible: continue
            audio_selected = compatible[0]
            tracks = [selected, audio_selected]
        actual_container = container
        if container == "auto": actual_container = "mp4" if _video_ok(selected, "mp4") and _audio_ok(audio_selected, "mp4") else "mkv"
        sizes = [f.get("filesize") or f.get("filesize_approx") for f in tracks]
        return {"ids": [f["format_id"] for f in tracks], "container": actual_container, "height": selected.get("height"), "vcodec": selected.get("vcodec"), "acodec": audio_selected.get("acodec"), "abr": audio_selected.get("abr"), "estimate": sum(sizes) if all(sizes) else None}
    raise WorkerError("format", "Qualità o contenitore non compatibile. Scegli un contenitore automatico o una qualità diversa.")


def _work(payload):
    path = Path(payload["directory"])
    if not path.is_absolute() or not path.is_dir(): raise WorkerError("destination", "Directory temporanea non disponibile.")
    return path.resolve()


def transfer(payload, emit, budget=None):
    work = _work(payload)
    if budget: budget.check()
    files, finished_bytes, last_emit = [], 0, 0.0
    format_id, resume, selected = None, ResumeStatus(), None

    def progress(data):
        nonlocal last_emit
        current = int(data.get("downloaded_bytes") or 0)
        timestamp = time.monotonic()
        if budget: budget.observe((str(work), format_id), current)
        if payload.get("byte_limit") is not None and finished_bytes + current > payload["byte_limit"]:
            raise WorkerError("network", "Budget byte della validazione superato.")
        if payload.get("deadline") is not None and timestamp >= payload["deadline"]:
            raise WorkerError("network", "Tempo della validazione superato.")
        if timestamp - last_emit < 0.2 and data.get("status") != "finished": return
        emit.send("progress", dict(downloaded=finished_bytes + current, total=selected["estimate"], speed=finite(data.get("speed")), eta=finite(data.get("eta")), detail=resume.detail(data)))
        last_emit = timestamp

    opts = dict(options(payload), outtmpl=str(work / "stream-%(format_id)s.%(ext)s"),
                progress_hooks=[progress], continuedl=True, postprocessors=[])
    # Keep the extractor's cookies, JS challenge state and HTTP context alive
    # until all selected streams have been downloaded.
    with yt_dlp.YoutubeDL(opts) as ydl:
        raw = ydl.extract_info(payload["url"], download=False, process=False)
        analysis = {**raw, "formats": [dict(f) for f in raw.get("formats", [])]}
        info = ydl.process_ie_result(analysis, download=False)
        if normalized(info)["live"]: raise WorkerError("source", "Le dirette non sono supportate.")
        selected = choose_formats(info, payload.get("options", {}))
        emit.send("phase", {"state": "transfer_ready"})
        for format_id in selected["ids"]:
            previous_size = sum(p.stat().st_size for p in work.glob(f"stream-{format_id}.*.part"))
            previous_index = 0
            for metadata in work.glob(f"stream-{format_id}.*.ytdl"):
                try: previous_index = int(json.loads(metadata.read_text('utf-8'))['downloader']['current_fragment']['index'])
                except (ValueError, KeyError, TypeError): pass
            resume = ResumeStatus(previous_size, previous_index)
            ydl.params['logger'] = QuietLogger(resume.message)
            ydl.params['format'] = format_id
            ydl.format_selector = ydl.build_format_selector(format_id)
            stream_info = {**raw, "formats": [dict(f) for f in info["formats"] if f["format_id"] == format_id]}
            downloaded = ydl.process_ie_result(stream_info, download=True)
            requests = downloaded.get("requested_downloads") or []
            path = Path((requests[0].get("filepath") if requests else None) or ydl.prepare_filename(downloaded)).resolve()
            if not path.is_relative_to(work) or not path.is_file(): raise WorkerError("engine", "Il motore non ha prodotto la traccia richiesta.")
            files.append(str(path))
            finished_bytes += path.stat().st_size
    return {"files": files, "selection": selected, "downloaded": finished_bytes}


def probe(binary: Path, file: Path, audio_only: bool):
    result = subprocess.run([str(binary / "ffprobe.exe"), "-v", "error", "-show_entries", "stream=codec_type,codec_name,width,height:format=duration,size", "-of", "json", str(file)], capture_output=True, timeout=30, creationflags=0x08000000)
    try: data = json.loads(result.stdout)
    except ValueError: raise WorkerError("engine", "Output multimediale non verificabile.") from None
    types = [s.get("codec_type") for s in data.get("streams", [])]
    duration = float(data.get("format", {}).get("duration", 0) or 0)
    if result.returncode or not file.is_file() or file.stat().st_size == 0 or not math.isfinite(duration) or duration <= 0 or "audio" not in types or (audio_only and "video" in types) or (not audio_only and "video" not in types):
        raise WorkerError("engine", "Il file finale non contiene le tracce audio/video attese.")
    return data


def finalize(payload, emit):
    work = _work(payload)
    binary = Path(payload["bin"]).resolve()
    files = [Path(p).resolve() for p in payload["files"]]
    if not files or len(files) > 2 or any(not p.is_relative_to(work) or not p.is_file() for p in files):
        raise WorkerError("destination", "Tracce temporanee non valide.")
    selected = payload["selection"]
    extension = selected["container"]
    if extension not in {"mp4", "mkv", "webm", "m4a", "mp3", "aac", "opus", "ogg", "wav", "flac"}: raise WorkerError("format", "Estensione non supportata.")
    output = work / f"verified.{extension}"
    args = [str(binary / "ffmpeg.exe"), "-nostdin", "-hide_banner", "-loglevel", "error", "-y"]
    for file in files: args += ["-i", str(file)]
    opts = payload.get("options", {})
    if opts.get("audio_only"):
        args += ["-map", "0:a:0", "-vn"]
        codec = opts.get("audio_format", "original")
        args += ["-c:a", "copy" if codec == "original" else "libmp3lame" if codec == "mp3" else "aac"]
        if codec != "original": args += ["-b:a", f"{int(opts.get('audio_bitrate', 192))}k"]
    else:
        args += ["-map", "0:v:0", "-map", "1:a:0" if len(files) == 2 else "0:a:0", "-c", "copy"]
    if extension in ("mp4", "m4a"): args += ["-movflags", "+faststart"]
    args += [str(output)]
    emit.send("phase", {"state": "muxing"})
    # ffmpeg error output goes to disk, not unbounded process memory or UI logs.
    diagnostic = work / "ffmpeg-diagnostic.txt"
    timeout = min(1800, max(1, payload.get('deadline', time.monotonic() + 1800) - time.monotonic()))
    with diagnostic.open("wb") as log:
        try: result = subprocess.run(args, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=log, timeout=timeout, creationflags=0x08000000)
        except subprocess.TimeoutExpired:
            raise WorkerError('network' if payload.get('deadline') else 'engine', 'Tempo massimo della finalizzazione superato.') from None
    if result.returncode:
        with diagnostic.open('rb') as log:
            log.seek(max(0, diagnostic.stat().st_size - 16384))
            failure = log.read().decode('utf-8', 'replace').lower()
        if any(text in failure for text in ('no space left', 'permission denied', 'access is denied', 'no such file or directory', 'disk full')):
            raise WorkerError('destination', 'Spazio insufficiente o cartella non accessibile durante la finalizzazione.')
        raise WorkerError("engine", "FFmpeg non ha completato il mux o la conversione.")
    verified = probe(binary, output, bool(opts.get("audio_only")))
    return {"verified_path": str(output), "probe": verified, "selection": selected}


def download(payload, emit):
    if payload.get("phase", "transfer") == "transfer": return transfer(payload, emit)
    if payload.get("phase") == "finalize": return finalize(payload, emit)
    if payload.get("phase") == "verify": return probe(Path(payload["bin"]), Path(payload["file"]), bool(payload.get("audio_only")))
    raise WorkerError("protocol", "Fase di download non valida.")
