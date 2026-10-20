"""Checked JSON boundaries shared by benchmark capture and comparison."""

from collections.abc import Mapping
import math
from typing import TypedDict


class BenchmarkMetadata(TypedDict):
    capture_policy: int
    runner_id: str
    host: str
    processor: str
    rustc: str
    environment: dict[str, str]
    cargo_config: dict[str, str]
    sha256: dict[str, str]
    command: list[str]
    median_ns: dict[str, float]
    sample_counts: dict[str, int]


def fields(value: object) -> dict[str, object]:
    if not isinstance(value, dict):
        raise ValueError("expected benchmark JSON object")
    result: dict[str, object] = {}
    for key, item in value.items():
        if not isinstance(key, str):
            raise ValueError("expected benchmark object string keys")
        result[key] = item
    return result


def strings(value: object) -> dict[str, str]:
    result: dict[str, str] = {}
    for key, item in fields(value).items():
        if not isinstance(item, str):
            raise ValueError("expected benchmark string mapping")
        result[key] = item
    return result


def positive(value: object) -> float:
    if (
        isinstance(value, bool)
        or not isinstance(value, (int, float))
        or not math.isfinite(value)
        or value <= 0
    ):
        raise ValueError("expected finite positive measurement")
    return value


def measurements(value: object) -> list[float]:
    if not isinstance(value, list):
        raise ValueError("expected benchmark measurement array")
    return [positive(item) for item in value]


def sample_values(value: object) -> tuple[list[float], list[float]]:
    sample = fields(value)
    iters = measurements(sample.get("iters"))
    times = measurements(sample.get("times"))
    if not iters or len(iters) != len(times):
        raise ValueError("incomplete raw samples")
    return iters, times


def cargo_options(metadata: Mapping[str, object]) -> list[str]:
    value = metadata.get("command")
    if not isinstance(value, list):
        raise ValueError("invalid benchmark command")
    command: list[str] = []
    for item in value:
        if not isinstance(item, str):
            raise ValueError("invalid benchmark command")
        command.append(item)
    if "--" not in command:
        raise ValueError("missing benchmark command separator")
    return command[: command.index("--")]
