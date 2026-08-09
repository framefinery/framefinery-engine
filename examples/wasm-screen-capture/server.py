#!/usr/bin/env python3
from __future__ import annotations

import datetime as dt
import json
import mimetypes
import os
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse


EXAMPLE_DIR = Path(__file__).resolve().parent
REPO_ROOT = EXAMPLE_DIR.parents[1]
DEFAULT_OUT_DIR = REPO_ROOT / "verification" / "generated" / "wasm_screen_capture"


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(EXAMPLE_DIR), **kwargs)

    def end_headers(self) -> None:
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def do_POST(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path != "/upload":
            self.send_error(404, "unknown upload endpoint")
            return

        length = int(self.headers.get("Content-Length", "0"))
        data = self.rfile.read(length)
        query = parse_qs(parsed.query)
        codec = query.get("codec", ["stream"])[0]
        frames = query.get("frames", ["unknown"])[0]
        suffix = ".obu" if codec == "av2" else ".vvc" if codec == "vvc" else ".bin"
        timestamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        out_dir = Path(os.environ.get("FRAMEFINERY_WASM_CAPTURE_DIR", DEFAULT_OUT_DIR))
        out_dir.mkdir(parents=True, exist_ok=True)
        output = out_dir / f"screen_capture_{codec}_{frames}f_{timestamp}{suffix}"
        output.write_bytes(data)

        body = json.dumps({"path": str(output), "bytes": len(data)}).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main() -> None:
    mimetypes.add_type("application/wasm", ".wasm")
    host = os.environ.get("FRAMEFINERY_WASM_HOST", "127.0.0.1")
    port = int(os.environ.get("FRAMEFINERY_WASM_PORT", "8008"))
    server = ThreadingHTTPServer((host, port), Handler)
    print(f"serving {EXAMPLE_DIR} at http://{host}:{port}/")
    print(f"uploads will be written under {DEFAULT_OUT_DIR}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print()


if __name__ == "__main__":
    main()
