// Disposable Windows runner only. No model calls or account credentials.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const fsp = fs.promises;
const path = require('node:path');
const https = require('node:https');
const { createHash } = require('node:crypto');
const { spawn } = require('node:child_process');
const { setTimeout: wait } = require('node:timers/promises');

const root = process.env.QA_ROOT;
const appDir = path.join(process.env.LOCALAPPDATA, 'AgenTeam');
const exe = path.join(appDir, 'AgenTeam.exe');
const profile = path.join(process.env.USERPROFILE, '.agenteam');
const workspace = path.join(process.env.USERPROFILE, 'AgenTeam upgrade QA workspace');
const evidence = path.join(root, 'evidence');
const session = 'dc517a77-2742-4b21-bf97-6b672be81261';
const project = '66c38ca1-7304-47c4-991c-889dddb53555';
const marker = '保留-0.6.0-' + process.env.GITHUB_RUN_ID;
const title = 'Saved conversation ' + marker;
const preference = 'myagents.globalSidebar.v1';
const fixturePaths = {
  transcript: path.join(profile,'sessions',session+'.jsonl'),
  skill: path.join(profile,'skills','qa-preserve','SKILL.md'),
  command: path.join(profile,'commands','qa-preserve.md'),
  document: path.join(workspace,'用户笔记.md'),
};
const packagedName = 'AgenTeam_0.6.1_x64-setup.exe';
const { chromium } = require(path.join(appDir, 'node_modules', 'playwright-core'));
const events = [];
function event(name, details) {
  events.push({time: new Date().toISOString(),name,details});
  fs.writeFileSync(path.join(evidence,'events.json'),JSON.stringify(events,null,2));
  console.log(name, typeof details === 'string' ? details : '');
}
async function digest(file) {
  const h = createHash('sha256');
  for await (const part of fs.createReadStream(file)) h.update(part);
  return h.digest('hex');
}
async function write(file, content) {
  await fsp.mkdir(path.dirname(file),{recursive:true});
  await fsp.writeFile(file,content);
}
async function json(file) {return JSON.parse((await fsp.readFile(file,'utf8')).replace(/^\uFEFF/,''));}
async function poll(label, fn, timeout=90000) {
  const deadline = Date.now()+timeout;
  let last = '';
  while (Date.now()<deadline) {
    let timer;
    try {
      const timeoutAttempt=new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('attempt exceeded 12 seconds')),12000);});
      const result=await Promise.race([fn(),timeoutAttempt]);
      if(result) return result;
    } catch(e) {last=String(e.message||e).slice(0,390);}
    finally {clearTimeout(timer);}
    await wait(850);
  }
  throw Error(label+' timed out. '+last);
}
async function seed() {
  assert(!fs.existsSync(fixturePaths.transcript),'fresh QA profile required');
  const now=new Date().toISOString();
  await write(path.join(profile,'config.json'), JSON.stringify({uiLanguage:'en-US',appearanceMode:'dark',minimizeToTray:false}));
  await write(path.join(profile,'projects.json'), JSON.stringify([{id:project,name:'Upgrade QA',displayName:'Upgrade QA',path:workspace,providerId:null,permissionMode:null,lastOpened:now}]));
  await write(path.join(profile,'sessions.json'), JSON.stringify([{id:session,title:'Initial fixture title',agentDir:workspace,createdAt:now,lastActiveAt:now}]));
  await write(fixturePaths.transcript, JSON.stringify({id:'9d5f44ad-7d40-45b7-af33-4c78c41b7796',role:'user',content:marker,timestamp:now})+'\n');
  await write(fixturePaths.skill,'---\nname: qa-preserve\ndescription: upgrade test\n---\n'+marker+'\n');
  await write(fixturePaths.command,'Summarize this test note: '+marker+'\n');
  await write(fixturePaths.document,'# 用户笔记\n'+marker+'\n');
}
function launch() {
  const child=spawn(exe,[],{cwd:appDir,windowsHide:true,stdio:'ignore'});
  child.unref();
  return child;
}
async function connect(version) {
  const browser=await poll('WebView2 loopback connection',()=>chromium.connectOverCDP('http://127.0.0.1:19222',{timeout:2500}));
  const page=await poll('rendered main desktop window '+version,async()=>{
    for(const context of browser.contexts()) for(const p of context.pages()){
      const id=await p.evaluate(async()=>{
        if(!window.__TAURI_INTERNALS__ || !document.querySelector('button'))return null;
        return {version:await window.__TAURI_INTERNALS__.invoke('plugin:app|version'),label:window.__TAURI_INTERNALS__.metadata?.currentWindow?.label};
      }).catch(()=>null);
      if(id?.version===version && id.label==='main') return p;
    }
    return null;
  });
  event('live desktop version',version);
  return {browser,page};
}
async function snapshot(page,stage) {
  const view=await poll('read user data with application '+stage,async()=>{
    const state=await page.evaluate(async({session,workspace,preference})=>{
      const invoke=window.__TAURI_INTERNALS__.invoke;
      const rows=await invoke('cmd_list_session_metadata',{agentDir:workspace});
      const row=rows.find(s=>s.id===session);
      const response=await invoke('global_sidecar_http_request',{request:{method:'GET',path:'/sessions/'+session}});
      const result=JSON.parse(response.body);
      const messages=result.session?.messages || [];
      const pref=JSON.parse(localStorage.getItem(preference)||'{}');
      return {title:row?.title,favorite:row?.favorite,content:messages.find(m=>m.role==='user')?.content,
        language:document.documentElement.lang,scheme:document.documentElement.dataset.colorScheme,
        width:pref.expandedWidth,webviewMarker:localStorage.getItem('agenteam-upgrade-qa')};
    },{session,workspace,preference});
    assert.equal(state.title,title); assert.equal(state.favorite,true); assert.equal(state.content,marker);
    assert.equal(state.language,'en-US'); assert.equal(state.scheme,'dark');
    assert.equal(state.webviewMarker,marker); assert.equal(state.width,311);
    return state;
  });
  const config=await json(path.join(profile,'config.json'));
  const projects=await json(path.join(profile,'projects.json'));
  const item=projects.find(p=>p.id===project);
  assert.equal(item.path,workspace);
  assert.equal(config.appearanceMode,'dark'); assert.equal(config.uiLanguage,'en-US');
  const hashes={};
  for (const [key,file] of Object.entries(fixturePaths)) hashes[key]=await digest(file);
  const data={project:{id:item.id,path:item.path,name:item.name},config:{appearanceMode:config.appearanceMode,uiLanguage:config.uiLanguage},view,hashes};
  await write(path.join(evidence,stage+'.json'),JSON.stringify(data,null,2));
  await page.screenshot({path:path.join(evidence,stage+'.png'),timeout:10000}).catch(()=>{});
  event('data verified',stage);
  return data;
}
async function feed(version='0.6.1') {
  const manifest=await json(path.join(root,'target','latest.json'));
  const name='AgenTeam_'+version+'_x64-setup.exe';
  assert.equal(manifest.version,version);
  const platform=manifest.platforms['windows-x86_64'];
  assert.equal(platform.url,'https://github.com/micsnq2yoma-lgtm/AgenTeam/releases/download/v'+version+'/'+name);
  assert.equal(platform.signature,(await fsp.readFile(path.join(root,'target',name+'.sig'),'utf8')).trim());
  platform.url='https://localhost:18443/'+name;
  const packagePath=path.join(root,version==='0.6.0'?'original':'target',name);
  const manifestBytes=Buffer.from(JSON.stringify(manifest));
  const served=[];
  function handle(req,res) {
    if(req.method!=='GET') {res.writeHead(405).end();return;}
    if(req.url==='/latest.json') {
      served.push({type:'manifest',time:Date.now()});
      res.writeHead(200,{'Content-Type':'application/json','Content-Length':manifestBytes.length}).end(manifestBytes);
    } else if(req.url==='/'+name) {
      served.push({type:'package',time:Date.now()});
      res.writeHead(200,{'Content-Type':'application/octet-stream','Content-Length':fs.statSync(packagePath).size});
      fs.createReadStream(packagePath).pipe(res);
    } else {res.writeHead(404).end();}
  }
  for(const host of ['127.0.0.1','::1']){
    const server=https.createServer({key:fs.readFileSync(path.join(root,'localhost.key')),cert:fs.readFileSync(path.join(root,'localhost.crt'))},handle);
    await new Promise((yes,no)=>server.once('error',no).listen(18443,host,yes));
    server.unref();
  }
  return served;
}
async function main(){
  assert(process.env.GITHUB_ACTIONS==='true' && process.env.RUNNER_OS==='Windows' && process.env.GITHUB_REPOSITORY==='micsnq2yoma-lgtm/AgenTeam');
  await fsp.mkdir(evidence,{recursive:true});
  await seed();
  const original=launch();
  const first=await connect('0.6.0');
  await poll('rename and favorite the conversation in the original running application',async()=>{
    const status=await first.page.evaluate(async({session,title})=>{
      const response=await window.__TAURI_INTERNALS__.invoke('global_sidecar_http_request',{
        request:{method:'PATCH',path:'/sessions/'+session,headers:{'content-type':'application/json'},
          body:JSON.stringify({title,titleSource:'user',favorite:true})}
      });
      return response.status;
    },{session,title});
    assert.equal(status,200);
    return true;
  });
  await first.page.evaluate(({key,marker})=>{
    const pref=JSON.parse(localStorage.getItem(key)||'{}');
    localStorage.setItem(key,JSON.stringify({...pref,version:1,preferredMode:'expanded',expandedWidth:311,expandedWorkspaceKeys:[],hasSeededDefaultExpansion:true,showAutomationSessions:false,sessionView:'all'}));
    localStorage.setItem('agenteam-upgrade-qa',marker);
  },{key:preference,marker});
  const before=await snapshot(first.page,'before-original-060');
  await first.page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('plugin:event|emit',{event:'tray:confirm-exit',payload:null})).catch(()=>{});
  await poll('clean exit of original 0.6.0',()=>original.exitCode!==null);
  if(process.argv.includes('--rehearse'))return;
  const shellPath=process.env.QA_BASELINE_SHELL;
  event('0.6.0 shell hashes',{original:await digest(exe),loopbackBuild:await digest(shellPath)});
  await fsp.copyFile(shellPath,exe);
  const preflight=process.argv.includes('--preflight');
  const served=await feed(preflight?'0.6.0':'0.6.1');
  const baseline=launch();
  const old=await connect('0.6.0');
  assert.deepEqual(await snapshot(old.page,'before-060-test-channel'),before);
  if(preflight){
    const hasUpdate=await old.page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('check_and_download_update'));
    assert.equal(hasUpdate,false);
    assert(served.some(x=>x.type==='manifest'));
    assert(!served.some(x=>x.type==='package'));
    event('native 0.6.0 updater read the trusted loopback HTTPS channel','0.6.0 already current');
    return;
  }
  const ready=old.page.getByRole('button',{name:/^(restart update|重启更新)$/i}).first();
  await ready.waitFor({state:'visible',timeout:180000});
  const expected=await digest(path.join(root,'target',packagedName));
  const pending=await digest(path.join(profile,'updates','pending_update.bin'));
  assert.equal(pending,expected);
  assert(served.some(x=>x.type==='package'));
  await old.page.screenshot({path:path.join(evidence,'update-ready.png')});
  event('0.6.1 downloaded by 0.6.0',pending);
  const priorReads=served.filter(x=>x.type==='manifest').length;
  await ready.click({timeout:15000});
  event('clicked the rendered Restart update button','0.6.0');
  const confirm=old.page.getByRole('button',{name:/^(exit|quit|退出)$/i}).last();
  if(await confirm.waitFor({timeout:5000}).then(()=>true,()=>false)) await confirm.click({timeout:5000}).catch(()=>{});
  await poll('0.6.0 updater exits for NSIS',()=>baseline.exitCode!==null,110000);
  assert(served.filter(x=>x.type==='manifest').length>priorReads,'expected native pre-install manifest recheck');
  await write(path.join(evidence,'feed-requests.json'),JSON.stringify(served,null,2));
  // The driver never runs the target installer. Prove that the updater replaced the installed binary.
  await poll('binary replacement by NSIS',async()=>await digest(exe)!==await digest(shellPath),180000);
  let latest;
  try {latest=await connect('0.6.1');event('automatic relaunch','0.6.1');}
  catch(firstError) {
    event('checking whether a separate launch is necessary',firstError.message);
    launch();latest=await connect('0.6.1');event('separate launch used after installation','0.6.1');
  }
  const after=await snapshot(latest.page,'after-061');
  assert.deepEqual(after,before);
  event('retention check passed',{installerSHA256:expected,installedApplicationSHA256:await digest(exe)});
}
main().then(()=>process.exit(0),err=>{console.error(err.stack||err);process.exit(1);});
