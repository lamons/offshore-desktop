"""Thin Nuitka entrypoint for the bundled offshore sidecar."""

import sys

from offshore.cli import app

if __name__ == "__main__":
    sys.exit(app())
