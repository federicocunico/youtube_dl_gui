"""Local-only fixture server used by Rust integration tests."""
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import sys

class Handler(SimpleHTTPRequestHandler):
    def log_message(self, *args): pass

server = ThreadingHTTPServer(('127.0.0.1', 0), partial(Handler, directory=sys.argv[1]))
print(server.server_port, flush=True)
server.serve_forever()
