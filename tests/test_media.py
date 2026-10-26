import contextlib
import functools
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "worker"))
import media
from protocol import Emitter, WorkerError
import io


def fixture_info():
    return {"id": "BaW_jenozKc", "title": "Test", "channel": "Fixture channel", "duration": 1,
            "formats": [
                {"format_id": "720", "vcodec": "avc1", "acodec": "none", "height": 720, "ext": "mp4"},
                {"format_id": "2160", "vcodec": "vp9", "acodec": "none", "height": 2160, "ext": "webm"},
                {"format_id": "aac", "vcodec": "none", "acodec": "mp4a.40.2", "abr": 128, "ext": "m4a"},
                {"format_id": "opus", "vcodec": "none", "acodec": "opus", "abr": 160, "ext": "webm"},
            ]}


class FormatTests(unittest.TestCase):
    def test_quality_cap_and_compatible_container(self):
        selection = media.choose_formats(fixture_info(), {"video_quality": 1080, "container": "mp4"})
        self.assertEqual(selection["ids"], ["720", "aac"])
        self.assertEqual(selection["height"], 720)
        self.assertEqual(media.choose_formats(fixture_info(), {})["container"], "mkv")
        with self.assertRaises(WorkerError): media.choose_formats(fixture_info(), {"video_quality": 360})

    def test_audio_only_does_not_select_video(self):
        original = media.choose_formats(fixture_info(), {"audio_only": True})
        self.assertEqual(original["ids"], ["opus"])
        self.assertEqual(original["container"], "webm")
        converted = media.choose_formats(fixture_info(), {"audio_only": True, "audio_format": "mp3"})
        self.assertEqual(converted["container"], "mp3")

    def test_metadata_has_no_signed_urls(self):
        info = fixture_info(); info["formats"][0]["url"] = "https://cdn.test/?token=private"
        result = media.normalized(info)
        self.assertIsNone(result["thumbnail"])
        self.assertNotIn("private", json.dumps(result))

    def test_inspect_never_downloads_and_rejects_live(self):
        binary = ROOT / "resources/runtime/bin"
        payload = {"bin": str(binary), "url": "https://youtube.com/watch?v=BaW_jenozKc"}
        with patch.object(media.yt_dlp, "YoutubeDL") as constructor:
            client = constructor.return_value.__enter__.return_value
            client.extract_info.return_value = fixture_info()
            result = media.inspect(payload, Emitter(io.BytesIO()))
            self.assertEqual(result["channel"], "Fixture channel")
            client.extract_info.assert_called_once_with(payload["url"], download=False)
            client.extract_info.return_value = {**fixture_info(), "is_live": True}
            with self.assertRaises(WorkerError): media.inspect(payload, Emitter(io.BytesIO()))

    def test_incremental_playlist_with_missing_entry(self):
        payload = {"bin": str(ROOT / "resources/runtime/bin"), "url": "https://youtube.com/playlist?list=fixture"}
        output = io.BytesIO()
        with patch.object(media.yt_dlp, "YoutubeDL") as constructor:
            constructor.return_value.__enter__.return_value.extract_info.return_value = {"entries": iter([fixture_info(), None])}
            self.assertEqual(media.enumerate_playlist(payload, Emitter(output))["count"], 2)
        events = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertTrue(events[0]["data"]["available"])
        self.assertFalse(events[1]["data"]["available"])

    def test_error_classification(self):
        self.assertEqual(media.classify(Exception("Private video")).code, "source")
        self.assertTrue(media.classify(Exception("HTTP Error 429")).retryable)
        self.assertEqual(media.classify(PermissionError("denied")).code, "destination")
        from urllib.error import HTTPError
        self.assertEqual(media.classify(HTTPError('http://localhost',429,'Too many requests',{'Retry-After':'45'},None)).retry_after,45)

    def test_resume_messages_distinguish_confirmed_resume_and_restart(self):
        status=media.ResumeStatus(1000)
        self.assertIn('Verifica',status.detail({'downloaded_bytes':1500}))
        status.message('[download] Resuming download at byte 1000')
        self.assertEqual(status.detail({}),'Ripresa dai dati parziali')
        status.message('[download] Unable to resume')
        self.assertIn('riavviato',status.detail({}))

    def test_validation_budget_accumulates_streams_and_restarts(self):
        import time
        budget=media.ByteBudget(100,time.monotonic()+10)
        budget.observe('video',40);budget.observe('video',50);budget.observe('audio',40)
        self.assertEqual(budget.bytes,90)
        with self.assertRaises(WorkerError):budget.observe('fallback-video',20)
        with self.assertRaises(WorkerError):media.ByteBudget(100,time.monotonic()-1).check()

    def test_disk_full_during_mux_is_destination_error(self):
        with tempfile.TemporaryDirectory() as directory:
            file=Path(directory)/'video.mp4';file.write_bytes(b'fixture')
            def failure(*args,**kwargs):
                kwargs['stderr'].write(b'No space left on device')
                return subprocess.CompletedProcess(args,1)
            with patch.object(media.subprocess,'run',side_effect=failure):
                with self.assertRaises(WorkerError) as result:media.finalize({'directory':directory,'bin':str(ROOT/'resources/runtime/bin'),'files':[str(file)],'selection':{'container':'mp4'}},Emitter(io.BytesIO()))
            self.assertEqual(result.exception.code,'destination')

    def test_large_playlist_stops_without_eager_consumption(self):
        consumed=[]
        def entries():
            for index in range(100000):
                consumed.append(index)
                yield fixture_info()
        class CancelledEmitter:
            def send(self,event,data):
                if len(consumed)==2: raise WorkerError('cancelled','stop')
        with patch.object(media.yt_dlp,'YoutubeDL') as constructor:
            constructor.return_value.__enter__.return_value.extract_info.return_value={'entries':entries()}
            with self.assertRaises(WorkerError): media.enumerate_playlist({'bin':str(ROOT/'resources/runtime/bin'),'url':'fixture'},CancelledEmitter())
        self.assertEqual(consumed,[0,1])


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *args): pass


class MediaIntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.directory = Path(cls.temp.name)
        cls.binary = ROOT / "resources/runtime/bin"
        args = [str(cls.binary / "ffmpeg.exe"), "-nostdin", "-v", "error", "-f", "lavfi", "-i", "testsrc=size=160x90:rate=10", "-f", "lavfi", "-i", "sine=frequency=440", "-t", "1", "-c:v", "mpeg4", "-pix_fmt", "yuv420p", "-c:a", "aac", "-f", "dash", str(cls.directory / "fixture.mpd")]
        subprocess.run(args, check=True, timeout=20, capture_output=True, cwd=cls.directory)
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(QuietHandler, directory=str(cls.directory)))
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True); cls.thread.start()
        cls.url = f"http://127.0.0.1:{cls.server.server_port}/fixture.mpd"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown(); cls.server.server_close(); cls.thread.join(); cls.temp.cleanup()

    def test_real_local_transfer_mux_audio_and_probe(self):
        for audio, codec in ((False, 'original'), (True, 'original'), (True, 'mp3'), (True, 'aac')):
            with self.subTest(audio=audio,codec=codec), tempfile.TemporaryDirectory() as directory:
                payload = {"bin": str(self.binary), "directory": directory, "url": self.url, "options": {"audio_only": audio, "audio_format": codec, "audio_bitrate":192}}
                transferred = media.transfer(payload, Emitter(io.BytesIO()))
                self.assertEqual(len(transferred["files"]), 1 if audio else 2)
                payload.update(transferred)
                result = media.finalize(payload, Emitter(io.BytesIO()))
                types = [s["codec_type"] for s in result["probe"]["streams"]]
                self.assertIn("audio", types)
                self.assertEqual("video" in types, not audio)

    def test_empty_and_missing_audio_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            empty = Path(directory) / "empty.mp4"; empty.touch()
            with self.assertRaises(WorkerError): media.probe(self.binary, empty, False)
            video = Path(directory) / "video.mp4"
            subprocess.run([str(self.binary / "ffmpeg.exe"), "-v", "error", "-f", "lavfi", "-i", "color=size=160x90", "-t", "1", "-an", "-c:v", "mpeg4", str(video)], check=True, capture_output=True, timeout=20)
            with self.assertRaises(WorkerError): media.probe(self.binary, video, False)


if __name__ == "__main__": unittest.main()
