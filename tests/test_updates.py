import io
import json
from pathlib import Path
import sys
import tempfile
import subprocess
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "worker"))
import update
from protocol import Emitter, WorkerError


class UpdateTests(unittest.TestCase):
    def test_stable_selection_crosses_year_and_epoch(self):
        files = [{"yanked": False}]
        metadata = {"releases": {"2026.10.21": files, "2027.1.1": files, "2028.1.1rc1": files, "2028.1.2": [{"yanked": True}], "1!1.0": files}}
        self.assertEqual(update.latest_stable(metadata), "1!1.0")

    def test_suite_nonempty_outputs_required_and_fallback_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); manifest=root/"canaries.json"
            manifest.write_text(json.dumps({"timeout_seconds":120,"byte_limit":26214400,"videos":[{"url":"first"},{"url":"second"}]}))
            payload={"canaries":str(manifest),"test_root":str(root/"tests"),"bin":str(ROOT/"resources/runtime/bin")}
            with patch.object(update.media,"inspect",return_value={"selection":{"ids":["v","a"]}}), patch.object(update.media,"transfer",return_value={"files":[],"selection":{},"downloaded":10}), patch.object(update.media,"finalize",side_effect=WorkerError("engine","Missing audio")):
                result=update.suite(payload,Emitter(io.BytesIO()))
            self.assertEqual(result["status"],"fail")
            self.assertEqual(result["control_url"],"first")
            self.assertEqual(list((root/"tests").iterdir()),[])

    def test_network_is_inconclusive_and_no_promotion(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); manifest=root/"canaries.json"
            manifest.write_text(json.dumps({"timeout_seconds":120,"byte_limit":26214400,"videos":[{"url":"first"},{"url":"second"}]}))
            with patch.object(update.media,"inspect",side_effect=WorkerError("network","offline")):
                result=update.suite({"canaries":str(manifest),"test_root":str(root/"tests"),"bin":"unused"},Emitter(io.BytesIO()))
            self.assertEqual(result["status"],"inconclusive")

    def test_stage_cannot_target_existing_or_external_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(WorkerError):
                update.prepare({"version":"2027.1.1","stage":directory,"staging_root":directory},Emitter(io.BytesIO()))

    def test_bad_wheel_hash_cannot_modify_baseline(self):
        baseline=ROOT/'resources/runtime/python'
        version_file=baseline/'Lib/site-packages/yt_dlp/version.py'
        before=version_file.read_bytes()
        with tempfile.TemporaryDirectory() as directory:
            parent=Path(directory);stage=parent/'candidate'
            def pip(python,arguments,cwd):
                if '--report' in arguments:
                    report=Path(arguments[arguments.index('--report')+1])
                    report.write_text(json.dumps({'install':[{'metadata':{'name':'yt-dlp','version':'2027.1.1'},'download_info':{'url':'https://files.pythonhosted.org/fixture.whl','archive_info':{'hashes':{'sha256':'0'*64}}}}]}),encoding='utf-8')
            with patch.object(update.shutil,'copytree',side_effect=lambda source,target:Path(target).mkdir()),patch.object(update,'pip_run',side_effect=pip),patch.object(update.urllib.request,'urlopen',return_value=io.BytesIO(b'not-the-expected-wheel')):
                with self.assertRaises(WorkerError) as result:update.prepare({'version':'2027.1.1','stage':str(stage),'staging_root':str(parent),'baseline_python':str(baseline)},Emitter(io.BytesIO()))
            self.assertEqual(result.exception.code,'runtime')
        self.assertEqual(version_file.read_bytes(),before)

    def test_python_requirement_failure_is_actionable(self):
        with patch.object(update.subprocess,'run',return_value=subprocess.CompletedProcess([],1,stdout=b'Package requires a different Python')):
            with self.assertRaises(WorkerError) as result:update.pip_run(ROOT/'resources/runtime/python/python.exe',['install'],ROOT)
        self.assertEqual(result.exception.code,'runtime')


if __name__ == "__main__": unittest.main()
