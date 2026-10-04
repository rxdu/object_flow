"""Run the studio's server: `python -m flowstudio` from `server/`.

It listens on the loopback address only, on the port `config.json` sets,
since it serves one user and holds only a sandbox store.
"""

import argparse
import json
import logging
import pathlib

from .http import serve

CONFIG = pathlib.Path(__file__).resolve().parents[2] / "config.json"


def main() -> None:
    config = json.loads(CONFIG.read_text())
    parser = argparse.ArgumentParser(prog="flowstudio")
    parser.add_argument("--port", type=int, default=int(config["server_port"]),
                        help="0 takes a free port, as a test does")
    args = parser.parse_args()
    host = "127.0.0.1"
    origin = f"http://127.0.0.1:{int(config['web_port'])}"
    logging.basicConfig(level=logging.INFO,
                        format="%(asctime)s %(levelname)s %(message)s")
    server = serve(host, args.port, origin)
    logging.getLogger("flowstudio").info(
        "listening on http://%s:%d, for %s", host, server.server_address[1],
        origin)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
