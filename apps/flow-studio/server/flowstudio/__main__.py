"""Run the studio's server: `python -m flowstudio` from `server/`.

It listens on the loopback address only, on the port `config.json` sets,
since it serves one user and holds only a sandbox store. Its wire is answered
by the backend `config.json` names, or `--backend`: `runner`, the reference
runner, or `core`, the Rust core's service, built and started as its child.
"""

import argparse
import json
import logging
import pathlib
import signal
import sys

from .core import Core, CoreUnavailable
from .http import serve

CONFIG = pathlib.Path(__file__).resolve().parents[2] / "config.json"


def main() -> None:
    config = json.loads(CONFIG.read_text())
    parser = argparse.ArgumentParser(prog="flowstudio")
    parser.add_argument("--port", type=int, default=int(config["server_port"]),
                        help="0 takes a free port, as a test does")
    parser.add_argument("--workspace", type=pathlib.Path,
                        default=CONFIG.parent / config["workspace"],
                        help="where the author's own flows are edited")
    parser.add_argument("--backend", choices=["runner", "core"],
                        default=config["backend"],
                        help="what answers the wire: the reference runner, "
                             "or the Rust core's service")
    args = parser.parse_args()
    host = "127.0.0.1"
    origin = f"http://127.0.0.1:{int(config['web_port'])}"
    logging.basicConfig(level=logging.INFO,
                        format="%(asctime)s %(levelname)s %(message)s")
    log = logging.getLogger("flowstudio")
    # a terminated server stops as an interrupted one does, closing the core's
    # service it started rather than leaving it running without a parent
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    core = None
    if args.backend == "core":
        try:
            core = Core()
        except CoreUnavailable as e:
            raise SystemExit(f"flowstudio: {e}") from None
    try:
        server = serve(host, args.port, args.workspace, origin, core=core)
        backend = f"the core at {core.url}" if core else "the reference runner"
        log.info("listening on http://%s:%d, for %s, editing %s, the wire "
                 "answered by %s", host, server.server_address[1], origin,
                 args.workspace, backend)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass
        finally:
            server.server_close()
    finally:
        if core:
            core.close()


if __name__ == "__main__":
    main()
