#!/usr/bin/env python3
import json,sys,os
if sys.argv[1:2]==['--version']:
 print('codex-cli 0.134.0');sys.exit(0)
if '--help' in sys.argv:
 print('Options:\n  --json\n  --model <MODEL>\n  -s, --sandbox <SANDBOX>\n  --output-schema <FILE>\n  --skip-git-repo-check\n  -c, --config <CONFIG>\n  -C, --cd <DIR>\n  --ephemeral\n  -o, --output-last-message <FILE>\n');sys.exit(0)
if sys.argv[1:2]==['app-server']:
 for line in sys.stdin:
  r=json.loads(line)
  if 'id' in r:
   result={'account':{'type':'chatgpt','email':'fixture@example.invalid'}} if r.get('method')=='account/read' else {}
   print(json.dumps({'id':r['id'],'result':result}),flush=True)
 sys.exit(0)
if sys.argv[1:2]==['login']:
 print('Logged in using ChatGPT');sys.exit(0)
r=sys.stdin.read()
if not r.lstrip().startswith('{'): kind='admission';msg='OK'
else:
 kind='worker';r=json.loads(r)
 source=[r['inputs']['request'][0]['artifact_id']]
 capture=r['inputs']['request'][0]['payload']
 result={'schema':'devrec/reconciliation-additions@1','source_artifact_ids':source,'request_digest':capture['request_digest'],'additions':[],'contradictions':[]}
 if capture['request']['records']:
  original=next(iter(capture['request']['records']))
  result['additions']=[{'schema_version':1,'id':'child','kind':'changelog','title':'Child','body':'Explicit child','depends_on':['parent']},{'schema_version':1,'id':'parent','kind':'changelog','title':'Parent','body':'Explicit parent','depends_on':[original]}]
 if os.path.exists(os.environ['CODEX_HOME']+'/bad-source'):
  result['source_artifact_ids']=['sha256:'+'0'*64]
 if os.path.exists(os.environ['CODEX_HOME']+'/two-sources'):
  result['source_artifact_ids']=source+[r['inputs']['requirements'][0]['artifact_id']]
 with open(os.environ['CODEX_HOME']+'/last-worker-input.json','w') as f:json.dump(r,f)
 with open(os.environ['CODEX_HOME']+'/last-reconciliation.json','w') as f:json.dump(result,f)
 msg=json.dumps({'schema':'af.worker-reply/1','outputs':{'reconciliation':[result]}})
if '-o' in sys.argv:
 with open(sys.argv[sys.argv.index('-o')+1],'w') as f:f.write(msg)
with open(os.environ['CODEX_HOME']+'/calls','a') as f:f.write(kind+'\n')
print(json.dumps({'type':'thread.started','thread_id':'fixture-'+kind}))
print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':msg}}))
print(json.dumps({'type':'turn.completed','usage':{'input_tokens':3,'cached_input_tokens':0,'output_tokens':2,'reasoning_output_tokens':0,'cache_write_input_tokens':0}}))
