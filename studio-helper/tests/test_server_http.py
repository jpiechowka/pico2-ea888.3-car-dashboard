import json,subprocess,time,urllib.request,urllib.error,socket,sys,threading,queue
from pathlib import Path
repo=Path.cwd()
child=subprocess.Popen([str(repo/'studio-helper/target/debug/studio-helper.exe'),'serve','--repo',str(repo)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
base='http://127.0.0.1:8765'
origin='http://127.0.0.1:8080'
def req(path,method='GET',headers=None):
 h={'Origin':origin};h.update(headers or {})
 r=urllib.request.Request(base+path,method=method,headers=h,data=b'' if method=='POST' else None)
 try:
  with urllib.request.urlopen(r,timeout=10) as response:return response.status,json.loads(response.read() or b'null')
 except urllib.error.HTTPError as e:return e.code,json.loads(e.read() or b'null')
try:
 startup=queue.Queue(maxsize=1)
 reader=threading.Thread(target=lambda: startup.put(child.stderr.readline()),daemon=True)
 reader.start()
 line=startup.get(timeout=5)
 reader.join(timeout=1)
 if not line.startswith(b'Studio helper listening at '):
  raise RuntimeError('Owned helper could not bind; no existing listener was contacted or terminated')
 status,session=req('/api/v1/connect','POST',{'X-Studio-Connect':'1'})
 assert status==200 and len(session['capability'])==64
 headers={'X-Studio-Capability':session['capability']}
 assert req('/api/v1/jobs')[0]==403
 assert req('/api/v1/connect','POST',{'Origin':'https://evil.example','X-Studio-Connect':'1'})[0]==403
 assert req('/api/v1/session',headers=headers)[1]['ready'] is False
 assert req('/api/v1/jobs',headers=headers)==(200,[])
 status,job=req('/api/v1/prerequisites/check','POST',headers)
 assert status==202,(status,job)
 assert req('/api/v1/prerequisites/check','POST',headers)[0]==409
 def poll(job):
  deadline=time.monotonic()+25
  while time.monotonic()<deadline:
   status,batch=req('/api/v1/jobs/'+job['job_id']+'/events?after=0&limit=128',headers=headers)
   assert status==200
   if batch['summary']['events_closed']:return batch
   time.sleep(.05)
  raise RuntimeError('Job did not close within bounded check window')
 batch=poll(job)
 assert batch['summary']['terminal_cause'] in ('failed','succeeded')
 assert batch['records'] and len(batch['records'])<=128
 status,report=req('/api/v1/prerequisites',headers=headers)
 assert status==200 and report['readiness_scope']=='installed_tool_versions_only'
 status,second=req('/api/v1/prerequisites/check','POST',headers)
 assert status==202
 assert req('/api/v1/jobs/'+second['job_id']+'/cancel','POST',headers)[0]==200
 assert poll(second)['summary']['terminal_cause']=='cancelled'
 assert req('/api/v1/builds','POST',headers)[0]==415
 if '--devices' in sys.argv:
  status,device_job=req('/api/v1/devices/refresh','POST',headers)
  assert status==202
  device_batch=poll(device_job)
  status,devices=req('/api/v1/devices',headers=headers)
  assert status==200 and devices['schema_version']==1 and len(devices['devices'])<=16
  assert all(not d['flash_ready'] and d['capacity']['state']=='unverified' and d['capacity']['bytes'] is None for d in devices['devices'])
  print('Read-only descriptor endpoint: complete=%s, candidates=%s; no flash-ready claims.'%(devices['complete'],len(devices['devices'])))
 print('Live HTTP: connect/auth, single active job, cached report, event polling, cancel/join and disabled build route passed.')
finally:
 child.terminate()
 _,stderr=child.communicate(timeout=10)
 if child.returncode is None:raise RuntimeError('Owned helper did not exit')
 print('Owned helper stopped and joined; no capability logged.')
