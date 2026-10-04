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
    parser.add_argument("--workspace", type=pathlib.Path,
                        default=CONFIG.parent / config["workspace"],
                        help="where the author's own flows are edited")
    args = parser.parse_args()
    host = "127.0.0.1"
    origin = f"http://127.0.0.1:{int(config['web_port'])}"
    logging.basicConfig(level=logging.INFO,
                        format="%(asctime)s %(levelname)s %(message)s")
    server = serve(host, args.port, args.workspace, origin)
    logging.getLogger("flowstudio").info(
        "listening on http://%s:%d, for %s, editing %s", host,
        server.server_address[1], origin, args.workspace)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
