"""Catalogue consumer parity; source checks complement real ARM compilation."""
import json
from pathlib import Path
import subprocess
import tomllib
import unittest

REPO=Path(__file__).resolve().parents[2]

class ClockProfiles(unittest.TestCase):
    def test_all_catalogue_aliases_are_present_and_use_their_exact_feature(self):
        output=subprocess.check_output(['cargo','run','--locked','--offline','-p','dashboard-config-tools','--','profiles'],cwd=REPO)
        rows=json.loads(output)['profiles']
        aliases=tomllib.loads((REPO/'firmware/.cargo/config.toml').read_text())['alias']
        manifest=tomllib.loads((REPO/'firmware/pico2/Cargo.toml').read_text())
        expected={a for row in rows for a in row['profile']['aliases'] if a.startswith('pico2')}
        ble_aliases = {'pico2-ble', 'pico2-ble-run', 'pico2-ble-280-70-1v30', 'pico2-ble-280-70-1v30-run'}
        self.assertEqual(set(aliases),expected | ble_aliases)
        self.assertEqual(len(expected),20)
        for row in rows:
            p=row['profile']
            self.assertIn(p['feature'],manifest['features'])
            self.assertNotIn(p['feature'],manifest['features']['default'])
            for alias in p['aliases']:
                if not alias.startswith('pico2-') or alias in ('pico2-run','pico2-fast','pico2-fast-run'): continue
                tokens=aliases[alias].split()
                self.assertEqual(tokens[0],'run' if alias.endswith('-run') else 'build')
                self.assertEqual(tokens[1:4],['-p','dashboard-pico2','--release'])
                self.assertEqual(set(tokens[-1].split(',')),{'simple-outline',p['feature']})
        for alias in ble_aliases:
            tokens = aliases[alias].split()
            clock = 'cpu280-spi70-1v30' if '280' in alias else 'cpu150-spi62-1v10'
            self.assertEqual(tokens[0], 'run' if alias.endswith('-run') else 'build')
            self.assertEqual(tokens[1:4], ['-p', 'dashboard-pico2', '--release'])
            self.assertEqual(set(tokens[-1].split(',')), {'simple-outline', 'ble-obd', clock})
        config = tomllib.loads((REPO/'firmware/.cargo/config.toml').read_text())
        self.assertEqual(config['build']['target'], 'thumbv8m.main-none-eabihf')
        self.assertIn('target-cpu=cortex-m33', config['target']['thumbv8m.main-none-eabihf']['rustflags'])

    def test_clock_adapter_preserves_peripheral_and_voltage_initialization(self):
        source=(REPO/'firmware/pico2/src/clock_profile.rs').read_text()
        self.assertIn('ClockConfig::crystal(12_000_000)',source)
        self.assertIn('PLL.is_valid(12_000_000)',source)
        self.assertIn('xosc.sys_pll = Some(PLL)',source)
        self.assertIn('clocks.core_voltage = CORE_VOLTAGE',source)
        for forbidden in ('system_freq(', 'manual_pll(', 'with_custom_pll(', 'disable_voltage', 'usb_pll =', 'peri_clk_src ='):
            self.assertNotIn(forbidden,source)
        main=(REPO/'firmware/pico2/src/main.rs').read_text()
        display=(REPO/'firmware/pico2/src/drivers/display.rs').read_text()
        self.assertNotIn('requested_cpu_mhz',main)
        self.assertNotIn('cfg(feature = "cpu',display)

if __name__=='__main__': unittest.main()
