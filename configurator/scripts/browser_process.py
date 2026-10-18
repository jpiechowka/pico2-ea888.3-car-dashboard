"""Compatibility import for the shared owned child-process implementation."""

from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "host/scripts"))
from owned_process import OwnedProcess

__all__ = ["OwnedProcess"]
