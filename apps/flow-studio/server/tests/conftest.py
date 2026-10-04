import sys
import threading
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(Path(__file__).resolve().parent))

from flowstudio.http import serve  # noqa: E402
from replay import Wire  # noqa: E402


@pytest.fixture(scope="session")
def wire():
    """The studio's server on a free loopback port, for the session."""
    server = serve("127.0.0.1", 0)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield Wire(f"http://127.0.0.1:{server.server_address[1]}")
    server.shutdown()
    server.server_close()
