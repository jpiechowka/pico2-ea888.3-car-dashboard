"""Validate version 1/2 decoded RTT groups. This measures software HAL completion,
not panel scanout or ECU support. Start a separate capture after every reboot.
"""

import argparse
import json
from pathlib import Path
import re
from collections.abc import Iterable
from typing import Literal, TypedDict, overload


class Quantile(TypedDict):
    lower: int
    upper: int


class Histogram(TypedDict):
    samples: int
    overflow: int
    rejected: int
    min: int | None
    max: int | None
    p50: Quantile | None
    p95: Quantile | None
    p99: Quantile | None
    unit: str


class GroupRows(TypedDict):
    inputs: list[int] | None
    metrics: dict[int, Histogram]
    pids: dict[int, list[int]]
    bad: bool
    record_version: int


class FrameGroup(GroupRows):
    kind: Literal[0]
    header: tuple[int, int, int, int, int, int | None, int]


class DiagnosticGroup(GroupRows):
    kind: Literal[1]
    header: tuple[int, int, int, int, int]


class FrameResult(TypedDict):
    record_version: int
    sequence: int
    start_us: int
    end_us: int
    frames: int
    payload_bytes: int
    active_cycles: int | None
    rejected: int
    inputs: list[int]
    metrics: list[Histogram]
    accepted_completion_rate_hz: float


class DiagnosticResult(TypedDict):
    sequence: int
    ecu: int
    epoch: int
    session_ms: int
    published_ms: int
    pids: dict[int, list[int]]
    metrics: dict[int, Histogram]


class ParseError(TypedDict):
    line: int | None
    reason: str


class MissingGroups(TypedDict):
    frames: int
    diagnostics: int


class TelemetryResult(TypedDict):
    frames: list[FrameResult]
    diagnostics: list[DiagnosticResult]
    errors: list[ParseError]
    missing_or_incomplete: MissingGroups
    scope: str


U32 = (1 << 32) - 1
U64 = (1 << 64) - 1
PIDS = {0, 32, 64, 5, 15, 66, 92}
KINDS = {0: "frames", 1: "diagnostics"}
RECORD = re.compile(r"\b(FRAME1|FRAME2|DIAG1|INPUT1|PID1|METRIC1|END1),(.*)$")
LENGTHS = {"FRAME1": 7, "FRAME2": 7, "DIAG1": 5, "INPUT1": 8, "PID1": 10, "METRIC1": 14, "END1": 2}


def require(condition: object, reason: str) -> None:
    if not condition:
        raise ValueError(reason)


def distribution(fields: list[int], kind: int) -> Histogram:
    n, overflow, rejected, low, high, *quantiles = fields
    bounds: list[Quantile | None]
    minimum: int | None = low
    maximum: int | None = high
    require(n <= U32 and overflow <= n and rejected <= U32, "invalid histogram counts")
    require(rejected == 0 or n == U32, "rejection before histogram exhaustion")
    if n == 0:
        require(all(x == 0 for x in fields), "nonempty statistics with zero samples")
        bounds = [None] * 3
        minimum = maximum = None
    else:
        require(low <= high, "minimum exceeds maximum")
        require(n != 1 or low == high, "one sample has different extrema")
        bounds = []
        by_rank: dict[int, tuple[int, int]] = {}
        for p, (a, b) in zip((50, 95, 99), zip(quantiles[::2], quantiles[1::2])):
            require(a <= b and max(a, low) <= min(b, high), "quantile outside observed range")
            rank = (n * p + 99) // 100
            require(
                rank not in by_rank or by_rank[rank] == (a, b),
                "equal nearest ranks have different bins",
            )
            by_rank[rank] = (a, b)
            require(rank != 1 or a <= low <= b, "first rank excludes minimum")
            require(rank != n or a <= high <= b, "last rank excludes maximum")
            in_overflow = rank > n - overflow
            if kind == 1 and in_overflow:
                require((a, b) == (8192, U64), "invalid compact overflow bin")
            else:
                width = 1 if a < 32 else 1 << (a.bit_length() - 5)
                require(a % width == 0 and b == a + width - 1, "invalid quantile bin")
                if kind == 1:
                    require(b < 8192, "compact bin out of range")
            bounds.append({"lower": a, "upper": b})
        require(
            quantiles[0] <= quantiles[2] <= quantiles[4]
            and quantiles[1] <= quantiles[3] <= quantiles[5],
            "unordered quantiles",
        )
        if kind == 1:
            require((overflow > 0) == (high >= 8192), "overflow disagrees with maximum")
            require((overflow == n) == (low >= 8192), "overflow disagrees with minimum")
    if kind == 0:
        require(overflow == rejected == 0, "frame histogram cannot overflow or reject internally")
    return dict(
        samples=n,
        overflow=overflow,
        rejected=rejected,
        min=minimum,
        max=maximum,
        p50=bounds[0],
        p95=bounds[1],
        p99=bounds[2],
        unit="us" if kind == 0 else "ms",
    )


@overload
def validate(
    group: FrameGroup,
    previous: FrameResult | None,
    prior_frame: bool = False,
    continuous_from_boot: bool = False,
) -> FrameResult: ...


@overload
def validate(
    group: DiagnosticGroup,
    previous: DiagnosticResult | None,
    prior_frame: bool = False,
    continuous_from_boot: bool = False,
) -> DiagnosticResult: ...


def validate(
    group: FrameGroup | DiagnosticGroup,
    previous: FrameResult | DiagnosticResult | None,
    prior_frame: bool = False,
    continuous_from_boot: bool = False,
) -> FrameResult | DiagnosticResult:
    kind = group["kind"]
    rows = group["metrics"]
    require(not group["bad"], "group contains corrupt or duplicate rows")
    require(set(rows) == (set(range(9)) if kind == 0 else PIDS), "missing or unknown metrics")
    if group["kind"] == 0:
        # A preceding result belongs to this same stream; reject mismatched API callers.
        if previous is not None and "ecu" in previous:
            raise ValueError("previous group kind mismatch")
        return validate_frame(group, previous, prior_frame, continuous_from_boot)
    if previous is not None and "record_version" in previous:
        raise ValueError("previous group kind mismatch")
    return validate_diagnostic(group, previous)


def validate_frame(
    group: FrameGroup, previous: FrameResult | None, prior_frame: bool, continuous_from_boot: bool
) -> FrameResult:
    rows = group["metrics"]
    seq, start, end, frames, payload, cycles, rejected = group["header"]
    require(end >= start and end - start >= 30_000_000, "invalid frame window duration")
    require(
        frames <= U32 and payload % 2 == 0 and 2 * frames <= payload <= 153600 * frames,
        "invalid completed frame count or payload",
    )
    require(frames != 0 or cycles == 0, "cycles without a completed frame")
    inputs = group["inputs"]
    if inputs is None:
        raise ValueError("missing input counters")
    require(all(m["samples"] <= frames for m in rows.values()), "too many metric samples")
    require(all(rows[i]["samples"] == frames for i in (1, 2, 3, 4)), "missing per-frame samples")
    require(
        rows[0]["samples"] == rows[8]["samples"] and rows[6]["samples"] == rows[7]["samples"],
        "mismatched interval/overlap or input batch counts",
    )
    intervals = rows[0]["samples"]
    expected = frames if prior_frame else max(0, frames - 1)
    if prior_frame or continuous_from_boot:
        require(intervals == expected, "missing or fabricated preceding frame interval")
    else:
        require(intervals in (frames, max(0, frames - 1)), "too many missing frame intervals")
    if previous:
        require(start >= previous["end_us"], "overlapping or regressed frame windows")
        if seq == previous["sequence"] + 1:
            require(start == previous["end_us"], "contiguous sequence has a time gap")
        require(
            all(a >= b for a, b in zip(inputs, previous["inputs"])),
            "input counters regressed",
        )
    return dict(
        record_version=group["record_version"],
        sequence=seq,
        start_us=start,
        end_us=end,
        frames=frames,
        payload_bytes=payload,
        active_cycles=cycles,
        rejected=rejected,
        inputs=inputs,
        metrics=[rows[i] for i in range(9)],
        accepted_completion_rate_hz=frames * 1_000_000 / (end - start),
    )


def validate_diagnostic(
    group: DiagnosticGroup, previous: DiagnosticResult | None
) -> DiagnosticResult:
    rows = group["metrics"]
    seq, ecu, epoch, at, published = group["header"]
    require(
        0x7E8 <= ecu <= 0x7EF and epoch != 0 and at <= published,
        "invalid ECU, epoch or diagnostic clock",
    )
    require(set(group["pids"]) == PIDS, "missing or unknown PID counters")
    for pid, counters in group["pids"].items():
        require(all(c <= U32 for c in counters), "PID counter out of range")
        requested, sent, completed, negative, timeouts, malformed, abandoned, faults = counters
        require(sent <= requested and faults <= abandoned, "impossible PID counts")
        replies = completed + negative
        require(
            min(completed + negative + timeouts + malformed + abandoned, U32) <= requested,
            "outcomes exceed requests",
        )
        require(min(replies, U32) <= sent, "reply without a transmitted request")
        require(rows[pid]["samples"] == min(replies, U32), "terminal reply and RTT counts disagree")
        # Individual outcome counters saturate independently. Their sum is a
        # lower bound after either saturates; histogram samples freeze at U32.
        rejected = rows[pid]["rejected"]
        require(rejected >= max(0, replies - U32), "missing exhausted-histogram rejections")
        if completed != U32 and negative != U32:
            require(rejected == max(0, replies - U32), "unexpected histogram rejections")
        if previous:
            old = previous["pids"][pid]
            require(all(a >= b for a, b in zip(counters, old)), "PID counters regressed")
            old_d = previous["metrics"][pid]
            new_d = rows[pid]
            require(
                new_d["samples"] >= old_d["samples"]
                and new_d["overflow"] >= old_d["overflow"]
                and new_d["rejected"] >= old_d["rejected"],
                "RTT counts regressed",
            )
            require(
                new_d["overflow"] - old_d["overflow"] <= new_d["samples"] - old_d["samples"],
                "too many new overflow samples",
            )
            if old_d["samples"]:
                require(
                    new_d["min"] is not None
                    and old_d["min"] is not None
                    and new_d["max"] is not None
                    and old_d["max"] is not None
                    and new_d["min"] <= old_d["min"]
                    and new_d["max"] >= old_d["max"],
                    "retained extrema contracted",
                )
            if new_d["samples"] == old_d["samples"]:
                require(
                    new_d["min"] == old_d["min"]
                    and new_d["max"] == old_d["max"]
                    and new_d["overflow"] == old_d["overflow"]
                    and new_d["p50"] == old_d["p50"]
                    and new_d["p95"] == old_d["p95"]
                    and new_d["p99"] == old_d["p99"],
                    "histogram changed without samples",
                )
    if previous:
        require(
            ecu == previous["ecu"]
            and epoch >= previous["epoch"]
            and at >= previous["session_ms"]
            and published >= previous["published_ms"],
            "diagnostic identity or clock regressed",
        )
    return dict(
        sequence=seq,
        ecu=ecu,
        epoch=epoch,
        session_ms=at,
        published_ms=published,
        pids=group["pids"],
        metrics=rows,
    )


def parse(lines: Iterable[str]) -> TelemetryResult:
    result: TelemetryResult = dict(
        frames=[],
        diagnostics=[],
        errors=[],
        missing_or_incomplete=dict(frames=0, diagnostics=0),
        scope="Software observations only. Split captures at reboot; no physical FPS or ECU qualification.",
    )
    active: dict[int, FrameGroup | DiagnosticGroup] = {}
    seen = {0: 0, 1: 0}
    last_frame: FrameResult | None = None
    last_diagnostic: DiagnosticResult | None = None
    prior_frame = False
    for number, line in enumerate(lines, 1):
        match = RECORD.search(line.rstrip("\r\n"))
        if not match:
            continue
        name, data = match.groups()
        try:
            require(len(data) <= 4096, "record too long")
            tokens = data.split(",")
            require(
                len(tokens) == LENGTHS[name]
                and all(
                    (name == "FRAME2" and i == 5 and x == "na") or re.fullmatch(r"[0-9]{1,20}", x)
                    for i, x in enumerate(tokens)
                ),
                "invalid integer record",
            )
            # Only FRAME2 cycle counts may be unavailable. All other positions are integers.
            values = [0 if x == "na" else int(x) for x in tokens]
            require(all(x <= U64 for x in values), "integer exceeds u64")
            if name in ("FRAME1", "FRAME2", "DIAG1"):
                kind = int(name == "DIAG1")
                seq = values[0]
                if kind in active:
                    result["errors"].append(
                        dict(
                            line=number,
                            reason=f"incomplete {KINDS[kind]} group {active[kind]['header'][0]}",
                        )
                    )
                    del active[kind]
                require(seq > seen[kind], "replayed/reset header; split capture after reboot")
                seen[kind] = seq
                if name == "DIAG1":
                    active[kind] = DiagnosticGroup(
                        kind=1,
                        header=(values[0], values[1], values[2], values[3], values[4]),
                        inputs=None,
                        metrics={},
                        pids={},
                        bad=False,
                        record_version=1,
                    )
                else:
                    active[kind] = FrameGroup(
                        kind=0,
                        header=(
                            values[0],
                            values[1],
                            values[2],
                            values[3],
                            values[4],
                            None if tokens[5] == "na" else values[5],
                            values[6],
                        ),
                        inputs=None,
                        metrics={},
                        pids={},
                        bad=False,
                        record_version=2 if name == "FRAME2" else 1,
                    )
                continue
            if name in ("METRIC1", "END1"):
                kind, seq, *rest = values
            else:
                kind = int(name == "PID1")
                seq, *rest = values
            require(kind in KINDS, "unknown group kind")
            require(
                kind in active and active[kind]["header"][0] == seq, "orphan or foreign sequence"
            )
            group = active[kind]
            if name == "END1":
                del active[kind]
                if group["kind"] == 0:
                    frame = validate(
                        group, last_frame, prior_frame, seq == len(result["frames"]) + 1
                    )
                    result["frames"].append(frame)
                    last_frame = frame
                    prior_frame = prior_frame or frame["frames"] != 0
                else:
                    diagnostic = validate(group, last_diagnostic)
                    result["diagnostics"].append(diagnostic)
                    last_diagnostic = diagnostic
            elif name == "INPUT1":
                require(group["inputs"] is None, "duplicate input counters")
                group["inputs"] = rest
            else:
                key, *fields = rest
                if name == "METRIC1":
                    require(key not in group["metrics"], "duplicate metric or PID")
                    group["metrics"][key] = distribution(fields, kind)
                else:
                    require(key not in group["pids"], "duplicate metric or PID")
                    group["pids"][key] = fields
        except ValueError as error:
            # A corrupted identity cannot reliably identify its owning group.
            # Conservatively invalidate all currently open groups.
            for group in active.values():
                group["bad"] = True
            result["errors"].append(dict(line=number, reason=str(error)))
    for kind, group in active.items():
        result["errors"].append(
            dict(line=None, reason=f"incomplete {KINDS[kind]} group {group['header'][0]} at EOF")
        )
    result["missing_or_incomplete"]["frames"] = seen[0] - len(result["frames"])
    result["missing_or_incomplete"]["diagnostics"] = seen[1] - len(result["diagnostics"])
    if not result["frames"] and not result["diagnostics"]:
        result["errors"].append(dict(line=None, reason="no complete recognized telemetry groups"))
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("capture", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    require(args.capture.resolve() != args.output.resolve(), "output must not overwrite capture")
    with args.capture.open(encoding="utf-8", errors="strict") as stream:
        result = parse(stream)
    args.output.write_text(json.dumps(result, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    print(
        f"Accepted {len(result['frames'])} frame windows and {len(result['diagnostics'])} diagnostic snapshots; {len(result['errors'])} errors"
    )
    return int(bool(result["errors"]))


if __name__ == "__main__":
    raise SystemExit(main())
