#!/usr/bin/env python3

import json
import os
import socket
import sys
from pathlib import Path
from typing import Optional


def config_dir() -> Optional[Path]:
    if sys.platform == "darwin":
        home = os.environ.get("HOME")
        if not home:
            return None
        return Path(home) / "Library" / "Application Support"

    xdg = os.environ.get("XDG_CONFIG_HOME")
    if xdg:
        return Path(xdg)

    home = os.environ.get("HOME")
    if not home:
        return None
    return Path(home) / ".config"


def hook_socket_path() -> Optional[Path]:
    base = config_dir()
    if base is None:
        return None
    return base / "glowmux" / "hooks.sock"


def main() -> int:
    pane_id = os.environ.get("GLOWMUX_PANE_ID")
    if not pane_id:
        return 0

    try:
        pane_id_value = int(pane_id)
    except ValueError:
        return 0

    raw = sys.stdin.read()
    if not raw.strip():
        return 0

    try:
        payload = json.loads(raw)
    except json.JSONDecodeError:
        return 0

    if not isinstance(payload, dict):
        return 0

    socket_path = hook_socket_path()
    if socket_path is None or not socket_path.exists():
        return 0

    payload["pane_id"] = pane_id_value
    encoded = json.dumps(payload, separators=(",", ":")).encode("utf-8")

    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.settimeout(0.2)
    try:
        client.connect(str(socket_path))
        client.sendall(encoded)
    except OSError:
        return 0
    finally:
        client.close()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
