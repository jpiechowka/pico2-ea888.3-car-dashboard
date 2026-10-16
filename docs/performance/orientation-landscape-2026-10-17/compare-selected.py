import sys,json,zipfile,hashlib
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[3]/'host/scripts'))
from compare_benchmarks import load,compatible
a_path,b_path=map(Path,sys.argv[1:3])
a_name,b_name=a_path.name,b_path.name
a,ar=load(a_path);b,br=load(b_path)
try:
    compatible(a,b)
except ValueError as e:
    assert str(e)=='fixture or capture source changed; review workload semantics separately',e
    strict=str(e)
else:
    strict='compatible'
assert ar.keys()==br.keys()
for k in ['runner_id','host','processor','rustc','rustc_path','environment','cargo_config']:
    assert a[k]==b[k],k
assert a['command'][:a['command'].index('--')]==b['command'][:b['command'].index('--')]
changed={p for p in set(a['sha256'])|set(b['sha256']) if p.startswith('host/benches/') and a['sha256'].get(p)!=b['sha256'].get(p)}
assert changed=={'host/benches/config_perf.rs','host/benches/rule_perf.rs','host/benches/view_perf.rs'},changed
with zipfile.ZipFile(a_path/'source.zip') as za, zipfile.ZipFile(b_path/'source.zip') as zb:
    def source(z,p):return z.read(p).decode().replace('\r\n','\n')
    old=source(za,'host/benches/view_perf.rs');new=source(zb,'host/benches/view_perf.rs')
    normalized=new.replace('    orientation(c);\n','',1).split('\nfn orientation(c: &mut Criterion) {')[0].rstrip()+'\n'
    assert old==normalized,'selected view fixture body changed'
    for p in ['host/benches/config_perf.rs','host/benches/rule_perf.rs']:
        old=source(za,p);new=source(zb,p).replace('                orientation: None,\n','')
        if p.endswith('config_perf.rs'):
            new=new.replace('Last field is the orientation enum. Reseal an invalid variant so','Last field is the backlight_off bool. Reseal its invalid encoding so').replace('*late.last_mut().unwrap() = 0xFF;','*late.last_mut().unwrap() = 2;')
        assert old==new,p
for p in ['host/Cargo.toml','host/Cargo.lock','host/rust-toolchain.toml','host/scripts/capture_benchmark.py','host/scripts/benchmark_environment.py','host/scripts/source_inventory.py']:
    assert a['sha256'][p]==b['sha256'][p],p
rows=[]
for name in sorted(ar):
    av,al,ah=ar[name];bv,bl,bh=br[name]
    rows.append(dict(workload=name,before_ns=av,before_ci95_ns=[al,ah],after_ns=bv,after_ci95_ns=[bl,bh],change_percent=(bv/av-1)*100,median_ci_overlap=not(bl>ah or al>bh)))
result=dict(before=a_name,after=b_name,strict_comparator=strict,selected_fixture_review='View benchmark body identical after removing unrelated new orientation group; other fixture changes confined to unselected config/rule migrations',rows=rows)
print(json.dumps(result,indent=2))
