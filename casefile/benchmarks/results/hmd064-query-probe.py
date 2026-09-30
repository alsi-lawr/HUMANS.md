import json,subprocess,tempfile,shutil,sqlite3,urllib.request
from pathlib import Path
repo=Path.cwd();scratch=repo/'.agent-workspace/20260930-speedup-implementation/writer/hmd064'
with tempfile.TemporaryDirectory(prefix='hmd064-sqlite-http-') as d:
 directory=Path(d);root=directory/'planning';db=directory/'index.sqlite'
 shutil.copytree(repo/'casefile/casefile-store/tests/fixtures/minimum',root)
 (root/'Unicode.md').write_text('İSTANBUL ΟΣ Straße 100% foo_bar left\0right')
 p=subprocess.Popen([str(repo/'.agent-workspace/20260930-speedup-implementation/writer/target/debug/casefile'),'--root',str(root),'serve','--index',str(db)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
 try:
  lines=[p.stdout.readline().rstrip() for _ in range(4)];base=lines[0].removeprefix('Casefile server: ');assert base.startswith('http://127.0.0.1:')
  def query(scope,search):
   request=urllib.request.Request(base+'/api/query',data=json.dumps({'query':'records','scope':scope,'search':search}).encode(),headers={'Content-Type':'application/json'})
   with urllib.request.urlopen(request,timeout=30) as response:
    assert response.status==200;return json.load(response)['Current']['value']
  records=query(None,'i\u0307stanbul');assert len(records)==1 and records[0]['path']=='Unicode.md'
  assert records[0]['content']=='İSTANBUL ΟΣ Straße 100% foo_bar left\0right'
  assert records[0]['rendered_markdown'] and 'İSTANBUL' in records[0]['rendered_markdown']
  assert query({'project':'demo','investigation':'sample'},'i\u0307stanbul')==[]
  assert query(None,'STRASSE')==[]
  assert len(query(None,'left\0right'))==1
  connection=sqlite3.connect(db)
  queries={
   'record':('SELECT path, document FROM records WHERE project = ? AND investigation IS ? AND identity = ? ORDER BY path LIMIT 2',('demo','sample','HMD-011')),
   'records_scope_search':('SELECT document FROM records WHERE project = ? AND investigation IS ? AND instr(search_text, ?) > 0 ORDER BY path',('demo','sample','minimum')),
   'boards':('SELECT document FROM boards WHERE project = ? AND investigation IS ? ORDER BY identity',('demo','sample')),
   'relationships':('SELECT document FROM relationships WHERE (source_project = ? AND source_investigation IS ? AND source_identity = ?) OR (target_project = ? AND target_investigation IS ? AND target_identity = ?) ORDER BY kind, source_identity, target_identity',('demo','sample','HMD-011','demo','sample','HMD-011'))}
  plans={name:[list(row) for row in connection.execute('EXPLAIN QUERY PLAN '+sql,args)] for name,(sql,args) in queries.items()}
  (scratch/'query-plan.json').write_text(json.dumps({'scope':'diagnostic only; no planner string assertions','sqlite_version':sqlite3.sqlite_version,'public_http_checks':'Unicode substring, NUL, nullable scope isolation and unchanged full rendered record passed','queries':{name:sql for name,(sql,args) in queries.items()},'plans':plans},indent=2)+'\n')
  connection.close();print('Actual native SQLite-backed HTTP Unicode/NUL/scope/full-display proof PASS; EXPLAIN captured diagnostically only')
 finally:
  p.terminate();p.wait(timeout=30)
