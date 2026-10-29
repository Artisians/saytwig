"""Private local protocol over the upstream Fermion Python speech API.

New application integration, not Fermion implementation code. Input configuration
arrives through the owned child's stdin; stdout carries one public port announcement.
No downloads, provider integration, transcript logging, or user audio files.
"""
import contextlib
import ctypes
from email import policy
from email.parser import BytesParser
import hashlib
import hmac
from http.server import BaseHTTPRequestHandler, HTTPServer
import io
import json
import os
from pathlib import Path
import socket
import socketserver
import sys
import threading
import time
import uuid
import wave

MODEL = "FermionResearch/Phonon-2.5-English"
WEIGHTS_SHA256 = "c57119c0f04756932acc1d60d90506346e3708d69f00e654084acc7fe507fe27"
LIMIT = 16_000 * 120
MAX_BODY = LIMIT * 2 + 16_384


def isolate_environment():
    permitted = {"PATH", "SYSTEMROOT", "WINDIR", "SYSTEMDRIVE", "COMSPEC", "HOME",
                 "USERPROFILE", "TEMP", "TMP", "TMPDIR", "APPDATA", "LOCALAPPDATA",
                 "PROGRAMDATA", "PROGRAMFILES", "PROGRAMFILES(X86)", "LANG", "LC_ALL", "LC_CTYPE"}
    for key in tuple(os.environ):
        if key.upper() not in permitted:
            del os.environ[key]
    os.environ.update(FERMION_DEVICE="cpu", FERMION_CPU_THREADS="4", HF_HUB_OFFLINE="1",
                      TRANSFORMERS_OFFLINE="1", HF_HUB_DISABLE_TELEMETRY="1",
                      DO_NOT_TRACK="1", TOKENIZERS_PARALLELISM="false")


def deny_outgoing(event, _arguments):
    if event in ("socket.connect", "socket.getaddrinfo"):
        raise OSError("Speech worker cannot initiate network connections")


def supervise(owner):
    if os.name == "nt":
        from ctypes import wintypes
        api = ctypes.WinDLL("kernel32", use_last_error=True)
        api.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
        api.OpenProcess.restype = wintypes.HANDLE
        api.WaitForSingleObject.argtypes = (wintypes.HANDLE, wintypes.DWORD)
        api.WaitForSingleObject.restype = wintypes.DWORD
        handle = api.OpenProcess(0x100000, False, owner)
        if not handle:
            raise RuntimeError("Owner is unavailable")
        def wait_for_exit():
            api.WaitForSingleObject(handle, 0xFFFFFFFF)
            os._exit(0)
    else:
        def wait_for_exit():
            while os.getppid() == owner:
                time.sleep(0.2)
            os._exit(0)
    threading.Thread(target=wait_for_exit, daemon=True).start()


def decode_form(content_type, body):
    if len(body) > MAX_BODY or "\r" in content_type or "\n" in content_type:
        raise ValueError("Invalid upload")
    envelope = ("Content-Type: " + content_type + "\r\nMIME-Version: 1.0\r\n\r\n").encode("ascii")
    message = BytesParser(policy=policy.default).parsebytes(envelope + body)
    if message.get_content_type() != "multipart/form-data" or not message.is_multipart():
        raise ValueError("Multipart form required")
    values = {}
    for part in message.iter_parts():
        name = part.get_param("name", header="content-disposition")
        if part.is_multipart() or name not in ("audio", "vocabulary") or name in values:
            raise ValueError("Invalid form fields")
        values[name] = part.get_payload(decode=True)
    words = json.loads(values.get("vocabulary", b"[]"))
    if (not isinstance(words, list) or len(words) > 25 or any(
            not isinstance(w, str) or not w or len(w) > 128 or any(ord(c) < 32 for c in w)
            for w in words)):
        raise ValueError("Invalid vocabulary")
    with wave.open(io.BytesIO(values.get("audio", b"")), "rb") as recording:
        if (recording.getnchannels(), recording.getframerate(), recording.getsampwidth(), recording.getcomptype()) != (1, 16000, 2, "NONE"):
            raise ValueError("Expected mono 16 kHz PCM16 audio")
        count = recording.getnframes()
        if not 0 < count <= LIMIT:
            raise ValueError("Recording length is outside the supported range")
        pcm = recording.readframes(count)
        if len(pcm) != count * 2:
            raise ValueError("Truncated audio")
    return pcm, words


def handler_type(engine, secret):
    class SpeechHandler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def setup(self):
            super().setup()
            self.connection.settimeout(15)

        def answer(self, status, payload, proof=None):
            encoded = json.dumps(payload, ensure_ascii=False).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(encoded)))
            if proof:
                self.send_header("X-Process-Proof", proof)
            self.end_headers()
            self.wfile.write(encoded)
            self.wfile.flush()

        def do_GET(self):
            if self.path != "/ready":
                self.close_connection = True
                return self.answer(404, {"error": "Unknown operation"})
            challenge = self.headers.get("X-Process-Challenge", "")
            try:
                if str(uuid.UUID(challenge)) != challenge:
                    raise ValueError()
            except ValueError:
                self.close_connection = True
                return self.answer(400, {"error": "Invalid challenge"})
            proof = hmac.digest(secret.encode(), (challenge + "|" + WEIGHTS_SHA256).encode(), "sha256").hex()
            return self.answer(200, {"protocol": 1, "model": MODEL, "weights": WEIGHTS_SHA256, "runtime": "0.2.15"}, proof)

        def do_POST(self):
            self.close_connection = True
            if self.path != "/transcribe":
                return self.answer(404, {"error": "Unknown operation"})
            if not hmac.compare_digest(self.headers.get("Authorization", ""), "Bearer " + secret):
                return self.answer(401, {"error": "Private session required"})
            try:
                lengths = self.headers.get_all("Content-Length", [])
                if len(lengths) != 1 or self.headers.get("Transfer-Encoding"):
                    raise ValueError()
                size = int(lengths[0])
                if not 0 < size <= MAX_BODY:
                    raise ValueError()
                body = self.rfile.read(size)
                if len(body) != size:
                    raise ValueError()
                pcm, vocabulary = decode_form(self.headers.get("Content-Type", ""), body)
            except (ValueError, KeyError, TypeError, EOFError, wave.Error):
                return self.answer(400, {"error": "Invalid recording or vocabulary"})
            try:
                import numpy as np
                audio = np.frombuffer(pcm, dtype="<i2").astype(np.float32) / 32768.0
                text, _, _ = engine.transcribe_array(audio, hotwords=vocabulary)
                if not isinstance(text, str) or "\0" in text or len(text.encode("utf-8")) > 131_072:
                    raise ValueError()
                return self.answer(200, {"text": text})
            except Exception:
                return self.answer(500, {"error": "Local speech decoding failed"})

        def log_message(self, *_args):
            pass
    return SpeechHandler


class LoopbackServer(HTTPServer):
    allow_reuse_address = False
    allow_reuse_port = False

    def server_bind(self):
        if os.name == "nt":
            self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = "localhost", self.server_address[1]

    def handle_error(self, *_args):
        # BaseServer otherwise logs a traceback that could include request data.
        pass


def main():
    configuration = json.loads(sys.stdin.buffer.readline(8193))
    secret = configuration["secret"]
    if not isinstance(secret, str) or str(uuid.UUID(secret)) != secret:
        raise ValueError("Invalid private session")
    owner = configuration["owner"]
    if not isinstance(owner, int) or owner <= 0:
        raise ValueError("Invalid owner")
    model = Path(configuration["model_directory"]).resolve(strict=True)
    supervise(owner)
    isolate_environment()
    sys.addaudithook(deny_outgoing)
    # Import/engine diagnostics are never retained alongside private dictation.
    with open(os.devnull, "w") as silence, contextlib.redirect_stdout(silence), contextlib.redirect_stderr(silence):
        from importlib.metadata import version
        if version("fermion-research") != "0.2.15":
            raise RuntimeError("Unexpected speech runtime")
        import fermion
        with (model / "model.fermion").open("rb") as file:
            if hashlib.file_digest(file, "sha256").hexdigest() != WEIGHTS_SHA256:
                raise RuntimeError("Incorrect model weights")
        engine = fermion.load_speech(str(model))
        server = LoopbackServer(("127.0.0.1", 0), handler_type(engine, secret))
    print(json.dumps({"port": server.server_port, "protocol": 1}), flush=True)
    with open(os.devnull, "w") as silence, contextlib.redirect_stdout(silence), contextlib.redirect_stderr(silence):
        server.serve_forever(poll_interval=0.2)


if __name__ == "__main__":
    try:
        main()
    except Exception:
        # Only the exit status crosses the private process boundary on failure.
        sys.exit(1)
