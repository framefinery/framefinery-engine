#!/usr/bin/env python3
from __future__ import annotations

import datetime as dt
import base64
import hashlib
import json
import mimetypes
import os
import struct
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse


EXAMPLE_DIR = Path(__file__).resolve().parent
REPO_ROOT = EXAMPLE_DIR.parents[1]
DEFAULT_OUT_DIR = REPO_ROOT / "verification" / "generated" / "wasm_screen_capture"
WEBSOCKET_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


class WebSocketProtocolError(Exception):
    pass


class Handler(SimpleHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(EXAMPLE_DIR), **kwargs)

    def end_headers(self) -> None:
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path == "/stream":
            self.handle_stream(parsed)
            return
        super().do_GET()

    def do_POST(self) -> None:
        parsed = urlparse(self.path)
        if parsed.path != "/upload":
            self.send_error(404, "unknown upload endpoint")
            return

        length = int(self.headers.get("Content-Length", "0"))
        data = self.rfile.read(length)
        query = parse_qs(parsed.query)
        codec = safe_token(query.get("codec", ["stream"])[0], "stream")
        frames = safe_token(query.get("frames", ["unknown"])[0], "unknown")
        suffix = suffix_for_codec(codec)
        timestamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        out_dir = capture_dir()
        out_dir.mkdir(parents=True, exist_ok=True)
        output = out_dir / f"screen_capture_{codec}_{frames}f_{timestamp}{suffix}"
        output.write_bytes(data)

        body = json.dumps({"path": str(output), "bytes": len(data)}).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def handle_stream(self, parsed) -> None:
        if self.headers.get("Upgrade", "").lower() != "websocket":
            self.send_error(400, "expected WebSocket upgrade")
            return
        if "upgrade" not in self.headers.get("Connection", "").lower():
            self.send_error(400, "expected Connection: Upgrade")
            return
        key = self.headers.get("Sec-WebSocket-Key")
        if not key:
            self.send_error(400, "missing Sec-WebSocket-Key")
            return

        query = parse_qs(parsed.query)
        codec = safe_token(query.get("codec", ["stream"])[0], "stream")
        suffix = suffix_for_codec(codec)
        timestamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        out_dir = capture_dir()
        out_dir.mkdir(parents=True, exist_ok=True)
        partial_output = out_dir / f"screen_capture_{codec}_stream_{timestamp}{suffix}.part"

        accept = base64.b64encode(hashlib.sha1((key + WEBSOCKET_GUID).encode("ascii")).digest())
        self.send_response(101, "Switching Protocols")
        self.send_header("Upgrade", "websocket")
        self.send_header("Connection", "Upgrade")
        self.send_header("Sec-WebSocket-Accept", accept.decode("ascii"))
        self.end_headers()
        self.close_connection = True

        bytes_written = 0
        frames = 0
        output = partial_output
        finished_cleanly = False
        closed_before_finish = False
        print(f"stream started: {partial_output}", flush=True)
        try:
            with partial_output.open("wb") as stream_file:
                self.send_ws_json({"type": "started", "path": str(partial_output)})
                while True:
                    message = self.read_ws_message()
                    if message is None:
                        closed_before_finish = True
                        break
                    opcode, payload = message
                    if opcode == 0x8:
                        self.send_ws_close()
                        closed_before_finish = True
                        break
                    if opcode == 0x9:
                        self.send_ws_frame(0xA, payload)
                        continue
                    if opcode == 0xA:
                        continue
                    if opcode == 0x2:
                        stream_file.write(payload)
                        stream_file.flush()
                        bytes_written += len(payload)
                        continue
                    if opcode == 0x1:
                        control = json.loads(payload.decode("utf-8"))
                        if control.get("type") != "finish":
                            raise WebSocketProtocolError(f"unsupported control message {control!r}")
                        frames = int(control.get("frames", 0))
                        declared_bytes = int(control.get("bytes", bytes_written))
                        if declared_bytes != bytes_written:
                            raise WebSocketProtocolError(
                                f"client declared {declared_bytes} byte(s), server received {bytes_written}"
                            )
                        stream_file.flush()
                        finished_cleanly = True
                        break
                    raise WebSocketProtocolError(f"unsupported WebSocket opcode {opcode}")
        except (BrokenPipeError, ConnectionError, EOFError):
            print(f"stream interrupted: {partial_output} ({bytes_written} byte(s))", flush=True)
            return
        except Exception as error:
            print(f"stream failed: {error}", flush=True)
            try:
                self.send_ws_json({"type": "error", "message": str(error)})
                self.send_ws_close(1011)
            except (BrokenPipeError, ConnectionError, EOFError):
                pass
            return

        if closed_before_finish and not finished_cleanly:
            print(f"stream interrupted: {partial_output} ({bytes_written} byte(s))", flush=True)
            return

        if finished_cleanly and bytes_written > 0:
            output = out_dir / f"screen_capture_{codec}_{frames}f_{timestamp}{suffix}"
            partial_output.replace(output)

        print(f"stream finished: {output} ({bytes_written} byte(s), {frames} frame(s))", flush=True)
        try:
            self.send_ws_json(
                {
                    "type": "finished",
                    "path": str(output),
                    "bytes": bytes_written,
                    "frames": frames,
                }
            )
            self.send_ws_close()
        except (BrokenPipeError, ConnectionError, EOFError):
            pass

    def read_ws_message(self):
        frame = self.read_ws_frame()
        if frame is None:
            return None
        fin, opcode, payload = frame
        if opcode in (0x8, 0x9, 0xA):
            return opcode, payload
        if opcode not in (0x1, 0x2):
            raise WebSocketProtocolError(f"unsupported WebSocket opcode {opcode}")
        if fin:
            return opcode, payload

        parts = [payload]
        while True:
            frame = self.read_ws_frame()
            if frame is None:
                raise EOFError("connection closed during fragmented message")
            fin, continuation_opcode, payload = frame
            if continuation_opcode in (0x8, 0x9, 0xA):
                raise WebSocketProtocolError("control frame inside fragmented message is not supported")
            if continuation_opcode != 0x0:
                raise WebSocketProtocolError("expected WebSocket continuation frame")
            parts.append(payload)
            if fin:
                return opcode, b"".join(parts)

    def read_ws_frame(self):
        header = self.rfile.read(2)
        if not header:
            return None
        if len(header) != 2:
            raise EOFError("truncated WebSocket frame header")
        first, second = header
        fin = bool(first & 0x80)
        opcode = first & 0x0F
        masked = bool(second & 0x80)
        length = second & 0x7F
        if length == 126:
            length = struct.unpack("!H", self.read_exact(2))[0]
        elif length == 127:
            length = struct.unpack("!Q", self.read_exact(8))[0]
        if not masked:
            raise WebSocketProtocolError("client WebSocket frames must be masked")
        if opcode in (0x8, 0x9, 0xA) and length > 125:
            raise WebSocketProtocolError("WebSocket control frame is too large")
        mask = self.read_exact(4)
        payload = bytearray(self.read_exact(length))
        for index, byte in enumerate(payload):
            payload[index] = byte ^ mask[index % 4]
        return fin, opcode, bytes(payload)

    def read_exact(self, length: int) -> bytes:
        data = self.rfile.read(length)
        if len(data) != length:
            raise EOFError("truncated WebSocket frame")
        return data

    def send_ws_json(self, message: dict) -> None:
        self.send_ws_frame(0x1, json.dumps(message).encode("utf-8"))

    def send_ws_close(self, code: int = 1000) -> None:
        self.send_ws_frame(0x8, struct.pack("!H", code))

    def send_ws_frame(self, opcode: int, payload: bytes = b"") -> None:
        first = 0x80 | opcode
        if len(payload) <= 125:
            header = struct.pack("!BB", first, len(payload))
        elif len(payload) <= 0xFFFF:
            header = struct.pack("!BBH", first, 126, len(payload))
        else:
            header = struct.pack("!BBQ", first, 127, len(payload))
        self.wfile.write(header)
        self.wfile.write(payload)
        self.wfile.flush()


def capture_dir() -> Path:
    return Path(os.environ.get("FRAMEFINERY_WASM_CAPTURE_DIR", DEFAULT_OUT_DIR))


def safe_token(value: str, fallback: str) -> str:
    cleaned = "".join(char for char in value if char.isalnum() or char in ("-", "_"))
    return cleaned or fallback


def suffix_for_codec(codec: str) -> str:
    return ".obu" if codec == "av2" else ".vvc" if codec == "vvc" else ".bin"


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
