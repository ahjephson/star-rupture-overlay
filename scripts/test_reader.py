# Optional manual live validation. Requires a running supported game; only reads memory.
import datetime,hashlib,json,pathlib,queue,shutil,subprocess,threading,time
R=pathlib.Path(__file__).resolve().parents[1];D=R/'local/reader-validation';D.mkdir(parents=True,exist_ok=True)
stage=R/'dist';exe=stage/'starrupture-overlay-reader.exe'
source=json.loads((R/'local/overlay.json').read_text(encoding='utf-8-sig'))
cache=D/'cache';cache.mkdir(exist_ok=True)
for name in ['session.json','live-context.json']:
 p=(R/'local'/source['cacheDirectory']).resolve()/name
 if p.exists():shutil.copy2(p,cache/name)
results=[]
def test(mode,seconds):
 log=(D/(mode+'-stderr.txt')).open('w',encoding='utf-8')
 p=subprocess.Popen([str(exe),'--cache-dir',str(cache)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,text=True,encoding='utf-8',bufsize=1,creationflags=0x08000000)
 q=queue.Queue()
 def read():
  for line in p.stdout:
   try:q.put(json.loads(line))
   except Exception as e:q.put({'parseError':str(e)})
 threading.Thread(target=read,daemon=True).start()
 p.stdin.write(json.dumps({'command':'start','version':1,'pause_interval_ms':50,'world_interval_ms':250})+'\n');p.stdin.flush()
 messages=[];ready=None;deadline=time.monotonic()+50;seq=0
 try:
  while time.monotonic()<deadline:
   m=q.get(timeout=8);messages.append(m)
   assert m.get('read_only') is True and m.get('game_code_invoked') is False and m['reader_pid']==p.pid,m
   assert m['sequence']==seq+1;seq=m['sequence']
   if m['kind']=='ready' and ready is None:ready=time.monotonic();deadline=ready+seconds
   if m['kind']=='unavailable':print(mode,'unavailable',m['data']['reason'],flush=True)
  assert ready is not None,'No ready message'
 finally:
  started=time.monotonic()
  if mode=='stop':p.stdin.write('{"command":"stop"}\n');p.stdin.flush()
  p.stdin.close()
  p.wait(timeout=2);elapsed=(time.monotonic()-started)*1000
  log.close()
  (D/(mode+'-messages.json')).write_text(json.dumps(messages),encoding='utf-8')
 world=[m for m in messages if m['kind']=='world'];pause=[m for m in messages if m['kind']=='pause']
 assert world and pause and p.returncode==0
 assert not [m for m in messages if m['kind']=='unavailable']
 stats=world[-1]['data']['statistics']
 if mode=='stop':assert stats['menuReads']>=80 and stats['worldReads']>=15,stats
 result={'mode':mode,'readerPid':p.pid,'messageCount':len(messages),'worldMessages':len(world),'menuMessages':len(pause),'generations':sorted(set(m['generation'] for m in messages)),'lastStatistics':stats,'shutdownMs':elapsed,'exitCode':p.returncode}
 results.append(result);print(json.dumps(result),flush=True)
test('stop',6)
test('eof',1)
(D/'validation.json').write_text(json.dumps({'status':'passed','cases':results,'activeServersChanged':False},indent=2),encoding='utf-8')

