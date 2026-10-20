"""Catalogue consumer parity; source checks complement real ARM compilation."""

from pathlib import Path
import tomllib
import unittest
from check_feature_profiles import clock_profiles
from script_values import array_items, object_fields, string_value

REPO = Path(__file__).resolve().parents[2]


class ClockProfiles(unittest.TestCase):
    def test_all_catalogue_aliases_are_present_and_use_their_exact_feature(self) -> None:
        rows = clock_profiles()
        config = object_fields(
            tomllib.loads((REPO / "firmware/.cargo/config.toml").read_text(encoding="utf-8")),
            "invalid Cargo config",
        )
        aliases = {
            alias: string_value(value, "alias must be a string")
            for alias, value in object_fields(config["alias"], "invalid aliases").items()
        }
        manifest = object_fields(
            tomllib.loads((REPO / "firmware/pico2/Cargo.toml").read_text(encoding="utf-8")),
            "invalid dashboard manifest",
        )
        feature_map = object_fields(manifest["features"], "invalid dashboard features")
        expected = {a for row in rows for a in row.aliases if a.startswith("pico2")}
        ble_aliases = {
            "pico2-ble",
            "pico2-ble-run",
            "pico2-ble-280-70-1v30",
            "pico2-ble-280-70-1v30-run",
        }
        probe_aliases: dict[str, list[str]] = {
            "ble-probe": [],
            "ble-probe-280-70-1v30": ["--features", "cpu280-spi70-1v30"],
        }
        self.assertEqual(set(aliases), expected | ble_aliases | probe_aliases.keys())
        self.assertEqual(len(expected), 20)
        for profile in rows:
            self.assertIn(profile.feature, feature_map)
            self.assertNotIn(
                profile.feature, array_items(feature_map["default"], "invalid default features")
            )
            for alias in profile.aliases:
                if not alias.startswith("pico2-") or alias in (
                    "pico2-run",
                    "pico2-fast",
                    "pico2-fast-run",
                ):
                    continue
                tokens = aliases[alias].split()
                self.assertEqual(tokens[0], "run" if alias.endswith("-run") else "build")
                self.assertEqual(tokens[1:4], ["-p", "dashboard-pico2", "--release"])
                self.assertEqual(set(tokens[-1].split(",")), {"simple-outline", profile.feature})
        for alias in ble_aliases:
            tokens = aliases[alias].split()
            clock = "cpu280-spi70-1v30" if "280" in alias else "cpu150-spi62-1v10"
            self.assertEqual(tokens[0], "run" if alias.endswith("-run") else "build")
            self.assertEqual(tokens[1:4], ["-p", "dashboard-pico2", "--release"])
            self.assertEqual(set(tokens[-1].split(",")), {"simple-outline", "ble-obd", clock})
        for alias, features in probe_aliases.items():
            # Utility aliases must build only; no inherited flashing runner.
            self.assertEqual(
                aliases[alias].split(),
                [
                    "build",
                    "-p",
                    "dashboard-ble-probe",
                    "--bin",
                    "ble-probe",
                    "--release",
                    *features,
                ],
            )
        probe = object_fields(
            tomllib.loads((REPO / "firmware/ble-probe/Cargo.toml").read_text(encoding="utf-8")),
            "invalid probe manifest",
        )
        probe_features = object_fields(probe["features"], "invalid probe features")
        self.assertEqual(probe_features["default"], [])
        self.assertEqual(probe_features["cpu280-spi70-1v30"], ["dashboard-pico2/cpu280-spi70-1v30"])
        self.assertEqual(
            object_fields(config["build"], "invalid build target")["target"],
            "thumbv8m.main-none-eabihf",
        )
        targets = object_fields(config["target"], "invalid Cargo targets")
        self.assertIn(
            "target-cpu=cortex-m33",
            array_items(
                object_fields(targets["thumbv8m.main-none-eabihf"], "invalid ARM target")[
                    "rustflags"
                ],
                "invalid ARM flags",
            ),
        )

    def test_clock_adapter_preserves_peripheral_and_voltage_initialization(self) -> None:
        source = (REPO / "firmware/pico2/src/clock_profile.rs").read_text()
        self.assertIn("ClockConfig::crystal(12_000_000)", source)
        self.assertIn("PLL.is_valid(12_000_000)", source)
        self.assertIn("xosc.sys_pll = Some(PLL)", source)
        self.assertIn("clocks.core_voltage = CORE_VOLTAGE", source)
        for forbidden in (
            "system_freq(",
            "manual_pll(",
            "with_custom_pll(",
            "disable_voltage",
            "usb_pll =",
            "peri_clk_src =",
        ):
            self.assertNotIn(forbidden, source)
        main = (REPO / "firmware/pico2/src/main.rs").read_text()
        display = (REPO / "firmware/pico2/src/drivers/display.rs").read_text()
        self.assertNotIn("requested_cpu_mhz", main)
        self.assertNotIn('cfg(feature = "cpu', display)


if __name__ == "__main__":
    unittest.main()
