"""Recheck the archived orientation migration without relaxing runner identity."""

import sys
import json
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "host/scripts"))
from compare_benchmarks import BUILD_INPUTS, OPTIONAL_RUNNERS, RUNNERS, load, compatible
from benchmark_records import cargo_options, strings

a_path, b_path = map(Path, sys.argv[1:3])
a_name, b_name = (a_path.name, b_path.name)
a, ar = load(a_path)
b, br = load(b_path)
a_hashes, b_hashes = (strings(a.get("sha256")), strings(b.get("sha256")))
try:
    compatible(a, b)
except ValueError as e:
    if str(e) != "fixture or capture source changed; review workload semantics separately":
        raise
    strict = str(e)
else:
    strict = "compatible"
if ar.keys() != br.keys():
    raise ValueError("selected comparison admission failed")
for k in ["runner_id", "host", "processor", "rustc", "rustc_path", "environment", "cargo_config"]:
    if a[k] != b[k]:
        raise ValueError(k)
if cargo_options(a) != cargo_options(b):
    raise ValueError("selected comparison admission failed")
changed = {
    p
    for p in set(a_hashes) | set(b_hashes)
    if p.startswith("host/benches/") and a_hashes.get(p) != b_hashes.get(p)
}
if changed != {
    "host/benches/config_perf.rs",
    "host/benches/rule_perf.rs",
    "host/benches/view_perf.rs",
}:
    raise ValueError(changed)
with zipfile.ZipFile(a_path / "source.zip") as za, zipfile.ZipFile(b_path / "source.zip") as zb:

    def source(z: zipfile.ZipFile, p: str) -> str:
        return z.read(p).decode().replace("\r\n", "\n")

    old = source(za, "host/benches/view_perf.rs")
    new = source(zb, "host/benches/view_perf.rs")
    normalized = (
        new.replace("    orientation(c);\n", "", 1)
        .split("\nfn orientation(c: &mut Criterion) {")[0]
        .rstrip()
        + "\n"
    )
    if old != normalized:
        raise ValueError("selected view fixture body changed")
    for p in ["host/benches/config_perf.rs", "host/benches/rule_perf.rs"]:
        old = source(za, p)
        new = source(zb, p).replace("                orientation: None,\n", "")
        if p.endswith("config_perf.rs"):
            new = new.replace(
                "Last field is the orientation enum. Reseal an invalid variant so",
                "Last field is the backlight_off bool. Reseal its invalid encoding so",
            ).replace("*late.last_mut().unwrap() = 0xFF;", "*late.last_mut().unwrap() = 2;")
        if old != new:
            raise ValueError(p)
for p in (*BUILD_INPUTS, *RUNNERS, *OPTIONAL_RUNNERS):
    if a_hashes.get(p) != b_hashes.get(p):
        raise ValueError(p)
rows = []
for name in sorted(ar):
    av, al, ah = ar[name]
    bv, bl, bh = br[name]
    rows.append(
        dict(
            workload=name,
            before_ns=av,
            before_ci95_ns=[al, ah],
            after_ns=bv,
            after_ci95_ns=[bl, bh],
            change_percent=(bv / av - 1) * 100,
            median_ci_overlap=not (bl > ah or al > bh),
        )
    )
result = dict(
    before=a_name,
    after=b_name,
    strict_comparator=strict,
    selected_fixture_review="View benchmark body identical after removing unrelated new orientation group; other fixture changes confined to unselected config/rule migrations",
    rows=rows,
)
print(json.dumps(result, indent=2))
