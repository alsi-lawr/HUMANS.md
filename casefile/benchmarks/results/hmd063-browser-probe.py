import os, subprocess, tempfile, shutil
from pathlib import Path
repo=Path.cwd()
with tempfile.TemporaryDirectory(prefix='hmd063-browser-') as d:
    directory=Path(d); root=directory/'planning'
    shutil.copytree(repo/'casefile/casefile-store/tests/fixtures/minimum',root)
    subprocess.run(['git','init','-q',str(root)],check=True)
    server=subprocess.Popen([str(repo/'.agent-workspace/20260930-speedup-implementation/writer/target/debug/casefile'),'--root',str(root),'serve','--write','--index',str(directory/'index.sqlite')],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    try:
        lines=[server.stdout.readline().rstrip() for _ in range(4)]
        base=lines[0].removeprefix('Casefile server: ')
        capability=lines[3].removeprefix('Casefile write capability: ')
        assert base.startswith('http://127.0.0.1:') and capability
        env=dict(os.environ,CASEFILE_PROBE_BASE=base,CASEFILE_PROBE_CAPABILITY=capability)
        subprocess.run(['bun',str(repo/'.agent-workspace/20260930-speedup-implementation/writer/hmd063/browser-probe.ts')],env=env,check=True)
    finally:
        server.terminate();server.wait(timeout=30)
