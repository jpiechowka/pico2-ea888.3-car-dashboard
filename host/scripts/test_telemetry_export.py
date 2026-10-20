"""Reject lost, mixed, corrupt and reset RTT groups before using any statistics."""

import unittest
from owned_process import run_checked
from pathlib import Path
from parse_telemetry import parse


def metric(kind: int, seq: int, key: int, samples: int = 1) -> str:
    return f"METRIC1,{kind},{seq},{key},{samples},0,0," + (
        "10,10,10,10,10,10,10,10" if samples else "0,0,0,0,0,0,0,0"
    )


def frame(seq: int = 1, start: int = 0, frames: int = 1) -> list[str]:
    return [
        f"FRAME1,{seq},{start},{start + 30000000},{frames},{frames * 2},0,0",
        f"INPUT1,{seq},0,0,0,0,0,0,0",
        *[
            metric(0, seq, i, max(0, frames - 1) if seq == 1 and i in (0, 8) else frames)
            for i in range(9)
        ],
        f"END1,0,{seq}",
    ]


def diagnostic(seq: int = 1, at: int = 1) -> list[str]:
    out = [f"DIAG1,{seq},2024,1,{at},{at + 1}"]
    for pid in [0, 32, 64, 5, 15, 66, 92]:
        out += [f"PID1,{seq},{pid},1,1,1,0,0,0,0,0", metric(1, seq, pid)]
    return out + [f"END1,1,{seq}"]


class Telemetry(unittest.TestCase):
    def test_unavailable_cycles_preserve_completed_frames_and_timing(self) -> None:
        rows = frame()
        rows[0] = "FRAME2,1,0,30000000,1,2,na,0"
        result = parse(rows)
        self.assertEqual(result["errors"], [])
        self.assertEqual(result["frames"][0]["frames"], 1)
        self.assertIsNone(result["frames"][0]["active_cycles"])
        self.assertEqual(result["frames"][0]["record_version"], 2)
        self.assertEqual(result["frames"][0]["metrics"][3]["max"], 10)
        self.assertEqual(parse(frame())["frames"][0]["active_cycles"], 0)
        self.assertEqual(parse(frame())["frames"][0]["record_version"], 1)
        for index in (1, 2, 3, 4, 5, 7):
            invalid = rows.copy()
            fields = invalid[0].split(",")
            fields[index] = "na"
            invalid[0] = ",".join(fields)
            self.assertFalse(parse(invalid)["frames"])
        rows[0] = "FRAME1,1,0,30000000,1,2,na,0"
        self.assertFalse(parse(rows)["frames"])

    def test_impossible_producer_states_and_unchanged_histograms(self) -> None:
        for replacement in [
            "PID1,1,0,1,0,1,0,0,0,0,0",
            metric(1, 1, 0, 0),
            "METRIC1,1,1,0,1,0,0,0,10,0,0,10,10,10,10",
            "DIAG1,1,0,0,1,2",
        ]:
            rows = diagnostic()
            index = next(
                i
                for i, row in enumerate(rows)
                if row.startswith(replacement.split(",")[0] + ",")
                and (row.startswith("DIAG1") or row.split(",")[1:3] == replacement.split(",")[1:3])
            )
            rows[index] = replacement
            r = parse(rows)
            self.assertFalse(r["diagnostics"], replacement)
            self.assertTrue(r["errors"])
        second = [
            row.replace(",10,10,10,10,10,10,10,10", ",20,20,20,20,20,20,20,20")
            for row in diagnostic(2, 2)
        ]
        self.assertEqual(len(parse(diagnostic() + second)["diagnostics"]), 1)
        second = frame(2, 30000000)
        second[2] = metric(0, 2, 0, 0)
        second[10] = metric(0, 2, 8, 0)
        self.assertEqual(len(parse(frame() + second)["frames"]), 1)
        self.assertEqual(
            parse(frame() + frame(2, 30000000)[:-1])["missing_or_incomplete"]["frames"], 1
        )
        self.assertTrue(parse(["unrecognized or empty"])["errors"])

    def test_actual_rust_producer_and_encoder_roundtrip(self) -> None:
        run = run_checked(
            ["cargo", "run", "--locked", "--quiet", "--example", "telemetry_fixture"],
            cwd=Path(__file__).resolve().parents[1],
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=True,
            timeout=300,
        )
        result = parse(run.stdout.splitlines())
        self.assertEqual(result["errors"], [])
        self.assertEqual([row["frames"] for row in result["frames"]], [1, 0])
        self.assertEqual(result["frames"][0]["metrics"][3]["max"], 10)
        self.assertIsNone(result["frames"][0]["active_cycles"])
        self.assertEqual(result["frames"][0]["record_version"], 2)
        self.assertEqual(result["diagnostics"][0]["metrics"][0]["max"], 2)
        self.assertEqual(result["diagnostics"][0]["pids"][0][:3], [1, 1, 1])

    def test_cumulative_growth_and_saturated_histogram_freeze(self) -> None:
        second = diagnostic(2, 2)
        second[1] = "PID1,2,0,2,2,2,0,0,0,0,0"
        second[2] = "METRIC1,1,2,0,2,0,0,10,20,10,10,20,20,20,20"
        self.assertEqual(parse(diagnostic() + second)["errors"], [])
        maximum = (1 << 32) - 1
        a, b = diagnostic(), diagnostic(2, 2)
        for seq, group, rejected in [(1, a, 0), (2, b, 1)]:
            group[1] = f"PID1,{seq},0,{maximum},{maximum},{maximum},0,0,0,0,0"
            group[2] = f"METRIC1,1,{seq},0,{maximum},0,{rejected},10,10,10,10,10,10,10,10"
        self.assertEqual(parse(a + b)["errors"], [])
        b[2] = b[2].replace(",10,10,10,10,10,10,10,10", ",20,20,20,20,20,20,20,20")
        self.assertEqual(len(parse(a + b)["diagnostics"]), 1)
        d = diagnostic()
        d[2] = d[2].replace(",1,0,0,10", ",1,0,1,10")
        self.assertFalse(parse(d)["diagnostics"])

    def test_first_completion_after_idle_or_lost_windows(self) -> None:
        first = frame(1, 0, 0)
        second = frame(2, 30000000)
        second[2] = metric(0, 2, 0, 0)
        second[10] = metric(0, 2, 8, 0)
        self.assertEqual(parse(first + second)["errors"], [])
        third = frame(3, 60000000)
        self.assertEqual(parse(first + second + third)["errors"], [])
        # A missing window could contain the first frame; either count is
        # possible until a complete captured window establishes a predecessor.
        self.assertEqual(parse(second)["errors"], [])
        self.assertEqual(parse(frame(2, 30000000))["errors"], [])

    def test_complete_interleaved_groups_and_empty_window(self) -> None:
        a, b = frame(), diagnostic()
        lines = ["irrelevant log"]
        for i in range(max(len(a), len(b))):
            if i < len(a):
                lines.append("0.000 INFO " + a[i])
            if i < len(b):
                lines.append(b[i])
        result = parse(lines + frame(2, 30000000, 0))
        self.assertEqual(result["errors"], [])
        self.assertEqual([len(result["frames"]), len(result["diagnostics"])], [2, 1])
        self.assertIsNone(result["frames"][1]["metrics"][0]["p99"])
        self.assertEqual(result["frames"][0]["metrics"][6]["unit"], "us")
        self.assertEqual(result["diagnostics"][0]["metrics"][0]["unit"], "ms")

    def test_every_missing_row_makes_a_group_unusable(self) -> None:
        for original in [frame(), diagnostic()]:
            for index in range(len(original)):
                result = parse(original[:index] + original[index + 1 :])
                self.assertEqual(result["frames"] + result["diagnostics"], [], index)
                self.assertTrue(result["errors"])

    def test_duplicates_foreign_sequences_replays_and_truncation(self) -> None:
        for group in [frame(), diagnostic()]:
            for i in range(len(group) - 1):
                bad = group[: i + 1] + [group[i]] + group[i + 1 :]
                r = parse(bad)
                self.assertTrue(r["errors"])
                self.assertEqual(r["frames"] + r["diagnostics"], [])
        bad = frame()
        bad[2] = metric(0, 99, 0)
        self.assertEqual(parse(bad)["frames"], [])
        self.assertTrue(parse(frame() + frame())["errors"])
        self.assertEqual(len(parse(frame() + frame())["frames"]), 1)
        self.assertEqual(
            parse(frame()[:-1] + frame(3, 60000000))["missing_or_incomplete"]["frames"], 2
        )

    def test_bad_numbers_clocks_counts_quantiles_and_unknown_rows(self) -> None:
        mutations = [
            ("FRAME1,1,0,30000000,1,2,0,0", "FRAME1,1,0,29999999,1,2,0,0"),
            ("INPUT1,1,0,0,0,0,0,0,0", "INPUT1,1,-1,0,0,0,0,0,0"),
            ("FRAME1,1,0,30000000,1,2,0,0", "FRAME1,1,0,30000000,1,3,0,0"),
            (
                "METRIC1,0,1,1,1,0,0,10,10,10,10,10,10,10,10",
                "METRIC1,0,1,1,2,0,0,10,10,11,10,10,10,10,10",
            ),
        ]
        for old, new in mutations:
            r = parse([line.replace(old, new) for line in frame()])
            self.assertFalse(r["frames"])
            self.assertTrue(r["errors"])
        for value in ["nan", "1.0", str(2**64), "+1", "1e3", "9" * 5000]:
            r = parse([frame()[0].replace("FRAME1,1,", "FRAME1," + value + ","), *frame()[1:]])
            self.assertFalse(r["frames"])
            self.assertTrue(r["errors"])

    def test_reset_and_lifetime_counter_regression_require_separate_capture(self) -> None:
        a = frame()
        a[1] = "INPUT1,1,3,0,0,0,0,0,0"
        r = parse(a + frame(2, 30000000))
        self.assertEqual(len(r["frames"]), 1)
        self.assertTrue(r["errors"])
        r = parse(diagnostic(2, 10) + diagnostic(3, 5))
        self.assertEqual(len(r["diagnostics"]), 1)
        self.assertTrue(r["errors"])
        r = parse(frame() + frame(3, 60000000) + diagnostic(4))
        self.assertEqual(r["missing_or_incomplete"], {"frames": 1, "diagnostics": 3})


if __name__ == "__main__":
    unittest.main()
