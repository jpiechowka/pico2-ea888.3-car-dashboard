"""Run the Rust browser-tests feature on an ephemeral loopback origin/profile.
Build first: trunk build --release --locked --features browser-tests --dist target/browser-tests
No WebDriver, JavaScript test implementation, browser extensions or global profile changes.
"""

import argparse
import socket
import http.server
import pathlib
import tempfile
import threading

from browser_process import OwnedProcess


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--browser", required=True)
    parser.add_argument("--dist", type=pathlib.Path, default=pathlib.Path("target/browser-tests"))
    parser.add_argument("--timeout", type=int, default=90)
    parser.add_argument(
        "--window-size",
        default="1440,1000",
        help="Owned browser viewport, e.g. 760,1000 for narrow layout",
    )
    args = parser.parse_args()
    done = threading.Event()
    results = []

    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *a, **kw):
            super().__init__(*a, directory=str(args.dist.resolve()), **kw)

        def log_message(self, *a):
            pass

        def do_GET(self):
            if self.path == "/__test_environment":
                self.send_response(200)
                self.end_headers()
                self.wfile.write(b"dashboard-studio-isolated-browser-tests-v1")
                return
            if self.path in ("/__disconnect", "/__partial"):
                if self.path == "/__partial":
                    self.send_response(200)
                    self.send_header("Content-Length", "100")
                    self.end_headers()
                    self.wfile.write(b"{")
                    self.wfile.flush()
                    threading.Event().wait(0.05)
                self.connection.shutdown(socket.SHUT_RDWR)
                self.connection.close()
                return
            if self.path == "/__silent_worker":
                self.send_response(200)
                self.send_header("Content-Type", "application/javascript")
                self.end_headers()
                return
            super().do_GET()

        def do_POST(self):
            if self.path not in ("/__result", "/__progress"):
                self.send_error(404)
                return
            length = int(self.headers.get("Content-Length", "0"))
            if not 0 < length < 8192:
                self.send_error(413)
                return
            message = self.rfile.read(length).decode("utf-8")
            print(message, flush=True)
            if self.path == "/__result":
                results.append(message)
            self.send_response(204)
            self.end_headers()
            if self.path == "/__result":
                done.set()

    with http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler) as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        with tempfile.TemporaryDirectory(prefix="studio-browser-test-") as profile:
            with tempfile.TemporaryFile() as log:
                process = OwnedProcess(
                    [
                        args.browser,
                        "--enable-logging=stderr",
                        "--headless=new",
                        "--disable-gpu",
                        "--no-first-run",
                        "--no-default-browser-check",
                        "--disable-background-networking",
                        "--disable-extensions",
                        "--window-size=" + args.window_size,
                        "--user-data-dir=" + profile,
                        "http://127.0.0.1:" + str(server.server_port) + "/?browser-tests=1",
                    ],
                    stdout=log,
                    stderr=log,
                )
                print(
                    f"START owned browser PID {process.pid}, isolated port {server.server_port}",
                    flush=True,
                )
                try:
                    completed = done.wait(args.timeout)
                finally:
                    process.cleanup()
                    server.shutdown()
                    thread.join(timeout=10)
                    print(
                        f"STOP owned browser PID {process.pid}, exit {process.returncode}",
                        flush=True,
                    )
                if not results:
                    print("FAIL browser suite deadline expired")
                if not completed or results != ["PASS browser failure suite"]:
                    log.seek(max(0, log.seek(0, 2) - 65536))
                    console = log.read().decode("utf-8", errors="replace")
                    print(
                        "\n".join(
                            line
                            for line in console.splitlines()
                            if "CONSOLE" in line or "browser test" in line
                        )[-12000:]
                    )
                    return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
