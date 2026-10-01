import json, subprocess, shutil, tempfile, os
from pathlib import Path
import jsonschema
repo=Path.cwd()
exe=repo/'.agent-workspace/20260930-speedup-implementation/writer/target/debug/casefile'
with tempfile.TemporaryDirectory(prefix='hmd063-schema-') as d:
    root=Path(d)/'planning'
    shutil.copytree(repo/'casefile/casefile-store/tests/fixtures/minimum',root)
    subprocess.run(['git','init','-q',str(root)],check=True)
    p=subprocess.Popen([str(exe),'mcp-package','--planning-root',str(root)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    def request(id,method,params):
        p.stdin.write(json.dumps({'jsonrpc':'2.0','id':id,'method':method,'params':params})+'\n');p.stdin.flush()
        result=json.loads(p.stdout.readline());assert result['id']==id,result
        return result
    request(1,'initialize',{'protocolVersion':'2025-06-18'})
    tools=request(2,'tools/list',{})['result']['tools']
    tool={t['name']:t for t in tools}
    path='projects/demo/investigations/sample/boards/probe.toml'
    preview=request(3,'tools/call',{'name':'casefile_preview_record','arguments':{'request':{'operation':'create','path':path,'draft':{'kind':'board','id':'HMD-probe','title':'Schema probe','status_source':'disposition','columns':[{'name':'Accepted','statuses':['accepted']}]}}}})
    assert not preview['result']['isError'],preview
    envelope=preview['result']['structuredContent']
    jsonschema.Draft202012Validator.check_schema(tool['casefile_preview_record']['outputSchema'])
    jsonschema.validate(envelope,tool['casefile_preview_record']['outputSchema'])
    old=request(4,'tools/call',{'name':'casefile_apply_record','arguments':{'preview_id':envelope['preview_id'],'canonical':{'request':{'operation':'delete','path':path}}}})
    assert old['result']['isError'] and not (root/path).exists(),old
    applied=request(5,'tools/call',{'name':'casefile_apply_record','arguments':{'preview_id':envelope['preview_id']}})
    assert not applied['result']['isError'],applied
    jsonschema.Draft202012Validator.check_schema(tool['casefile_apply_record']['outputSchema'])
    jsonschema.validate(applied['result']['structuredContent'],tool['casefile_apply_record']['outputSchema'])
    p.stdin.close();returncode=p.wait(timeout=30);assert returncode==0,p.stderr.read()
    result={'tool_declarations':[tool['casefile_preview_record'],tool['casefile_apply_record']],'preview_response':preview,'obsolete_response':old,'apply_response':applied}
    (repo/'.agent-workspace/20260930-speedup-implementation/writer/hmd063/mcp-id-schema.json').write_text(json.dumps(result,indent=2)+'\n')
    print('native MCP tools/list + compact preview/apply schema validation PASS; obsolete body refused; dated transport 2025-06-18 unchanged')
