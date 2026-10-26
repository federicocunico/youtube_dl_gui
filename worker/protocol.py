"""Versioned, bounded JSON Lines boundary between Rust and the private worker."""
from __future__ import annotations

import json
import re
from dataclasses import dataclass
from typing import Any, BinaryIO, Callable

VERSION = 1
MAX_REQUEST = 1024 * 1024
MAX_EVENT = 2 * 1024 * 1024
ID = re.compile(r"^[A-Za-z0-9_-]{1,80}$")


class WorkerError(Exception):
    def __init__(self, code: str, message: str, retryable: bool = False, retry_after: int | None = None):
        super().__init__(message)
        self.code = code
        self.retryable = retryable
        self.retry_after = retry_after


def redact(message: str) -> str:
    message = re.sub(r"https?://[^\s<>\"']+", "[URL rimossa]", str(message))
    message = re.sub(r"(?i)(authorization|cookie|token|password)\s*[:=]\s*\S+", r"\1=[redatto]", message)
    return message[:2000]


@dataclass(frozen=True)
class Request:
    job_id: str
    attempt_id: str
    command: str
    payload: dict[str, Any]

    @classmethod
    def read(cls, stream: BinaryIO) -> "Request":
        line = stream.readline(MAX_REQUEST + 1)
        if not line or len(line) > MAX_REQUEST:
            raise WorkerError("protocol", "Richiesta mancante o troppo grande.")
        try:
            value = json.loads(line)
        except (ValueError, UnicodeError):
            raise WorkerError("protocol", "JSON della richiesta non valido.") from None
        if not isinstance(value, dict) or type(value.get("protocol")) is not int or value["protocol"] != VERSION:
            raise WorkerError("protocol", "Versione del protocollo non supportata.")
        for field in ("job_id", "attempt_id"):
            if not isinstance(value.get(field), str) or not ID.fullmatch(value[field]):
                raise WorkerError("protocol", f"Identificatore {field} non valido.")
        if not isinstance(value.get("command"), str) or value["command"] not in {"healthcheck", "inspect", "enumerate", "download"}:
            raise WorkerError("protocol", "Comando non supportato.")
        if not isinstance(value.get("payload", {}), dict):
            raise WorkerError("protocol", "Payload non valido.")
        return cls(value["job_id"], value["attempt_id"], value["command"], value.get("payload", {}))


class Emitter:
    def __init__(self, stream: BinaryIO, request: Request | None = None):
        self.stream = stream
        self.request = request

    def send(self, event: str, data: Any) -> None:
        envelope = {"protocol": VERSION, "job_id": self.request.job_id if self.request else None,
                    "attempt_id": self.request.attempt_id if self.request else None,
                    "event": event, "data": data}
        try:
            encoded = json.dumps(envelope, ensure_ascii=False, allow_nan=False, separators=(",", ":")).encode("utf-8") + b"\n"
        except (ValueError, TypeError):
            raise WorkerError("protocol", "Evento non serializzabile.") from None
        if len(encoded) > MAX_EVENT:
            raise WorkerError("protocol", "Evento troppo grande.")
        self.stream.write(encoded)
        self.stream.flush()


def serve(source: BinaryIO, destination: BinaryIO,
          handler: Callable[[Request, Emitter], Any], engine_version: str) -> int:
    emitter = Emitter(destination)
    try:
        request = Request.read(source)
        emitter.request = request
        emitter.send("hello", {"engine": engine_version, "protocol": VERSION})
        result = handler(request, emitter)
        emitter.send("result", result)
        return 0
    except WorkerError as error:
        emitter.send("error", {"code": error.code, "message": redact(str(error)), "retryable": error.retryable, "retry_after": error.retry_after})
    except Exception as error:
        emitter.send("error", {"code": "engine", "message": redact(f"Errore del motore: {type(error).__name__}: {error}"), "retryable": False})
    return 1
