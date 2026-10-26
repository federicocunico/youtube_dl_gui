import io
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "worker"))
from protocol import Emitter, MAX_EVENT, MAX_REQUEST, Request, WorkerError, redact, serve


def request(**changes):
    value = {"protocol": 1, "job_id": "job-1", "attempt_id": "attempt-2", "command": "healthcheck", "payload": {}}
    value.update(changes)
    return json.dumps(value).encode() + b"\n"


class ProtocolTests(unittest.TestCase):
    def test_subprocess_roundtrip(self):
        result = subprocess.run([sys.executable, "-I", "-u", str(ROOT / "worker/main.py")],
                                input=request(), capture_output=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        events = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual([e["event"] for e in events], ["hello", "result"])
        self.assertTrue(all(e["job_id"] == "job-1" and e["attempt_id"] == "attempt-2" for e in events))
        self.assertEqual(Path(events[1]["data"]["executable"]), Path(sys.executable))

    def test_malformed_requests(self):
        for data in (b"", b"{no json}\n", b"[]\n", request(protocol=2), request(protocol=True),
                     request(job_id="../../escape"), request(payload=[]), request(command="shell"), b"x" * (MAX_REQUEST + 1)):
            with self.subTest(data=data[:100]), self.assertRaises(WorkerError):
                Request.read(io.BytesIO(data))

    def test_bounded_output(self):
        output = io.BytesIO()
        with self.assertRaises(WorkerError):
            Emitter(output).send("result", "x" * MAX_EVENT)
        self.assertEqual(output.getvalue(), b"")

    def test_handler_crash_is_structured_and_redacted(self):
        def crash(req, emit):
            raise RuntimeError("failed https://example.org/private?token=secret")
        output = io.BytesIO()
        self.assertEqual(serve(io.BytesIO(request()), output, crash, "test"), 1)
        error = json.loads(output.getvalue().splitlines()[-1])
        self.assertEqual(error["event"], "error")
        self.assertEqual(error["data"]["code"], "engine")
        self.assertNotIn("secret", error["data"]["message"])

    def test_oversized_result_is_protocol_error(self):
        output = io.BytesIO()
        self.assertEqual(serve(io.BytesIO(request()), output, lambda r, e: "x" * MAX_EVENT, "test"), 1)
        self.assertEqual(json.loads(output.getvalue().splitlines()[-1])["data"]["code"], "protocol")

    def test_nonfinite_number_is_rejected(self):
        with self.assertRaises(WorkerError):
            Emitter(io.BytesIO()).send("result", float("nan"))

    def test_redaction(self):
        self.assertNotIn("private", redact("Authorization: private"))


if __name__ == "__main__":
    unittest.main()
