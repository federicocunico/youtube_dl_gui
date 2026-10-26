"""Entry point, always launched by absolute path with private Python -I -u."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from protocol import Emitter, Request, WorkerError, serve


def dispatch(request: Request, emitter: Emitter):
    if request.command == "healthcheck":
        import packaging.version
        import yt_dlp.version
        return {"engine": yt_dlp.version.__version__, "python": sys.version.split()[0],
                "executable": sys.executable, "protocol": 1}
    import media
    try:
        if request.command == "inspect": return media.inspect(request.payload, emitter)
        if request.command == "enumerate": return media.enumerate_playlist(request.payload, emitter)
        if request.command == "download": return media.download(request.payload, emitter)
        raise WorkerError("protocol", "Comando non supportato.")
    except Exception as error:
        raise media.classify(error) from None


def main() -> int:
    try:
        from yt_dlp.version import __version__
    except ImportError:
        sys.stdout.buffer.write(b'{"protocol":1,"job_id":null,"attempt_id":null,"event":"error","data":{"code":"runtime","message":"Motore Python mancante.","retryable":false}}\n')
        return 1
    return serve(sys.stdin.buffer, sys.stdout.buffer, dispatch, __version__)


if __name__ == "__main__":
    raise SystemExit(main())
