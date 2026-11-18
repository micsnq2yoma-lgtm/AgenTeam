// Disposable Windows runner only. No model calls or account credentials.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const fsp = fs.promises;
const path = require('node:path');

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
  await page.setViewportSize({width:1360,height:800});
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
    assert.equal(state.webviewMarker,marker); assert.equal(state.width,480);
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
async function main(){
  assert(process.env.GITHUB_ACTIONS==='true' && process.env.RUNNER_OS==='Windows' && process.env.GITHUB_REPOSITORY==='micsnq2yoma-lgtm/AgenTeam');
  await fsp.mkdir(evidence,{recursive:true});
  await seed();
  const originalHash = '76f69af0386b90aacdfa5fc23e46ff13048ff64d77a3de40455fb4b7be0069ac';
  assert.equal(await digest(exe),originalHash);
  event('unmodified 0.6.0 installed binary SHA-256',originalHash);
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
  const resize=first.page.locator('[role="separator"][aria-valuemax="480"]');
  await resize.waitFor({state:'visible',timeout:30000});
  await resize.press('End');
  await first.page.waitForFunction(key=>JSON.parse(localStorage.getItem(key)||'{}').expandedWidth===480,preference);
  await first.page.evaluate(value=>localStorage.setItem('agenteam-upgrade-qa',value),marker);
  const before=await snapshot(first.page,'before-original-060');

  // Wait for the released binary's normal startup check and desktop update control.
  const ready=first.page.getByRole('button',{name:/^(restart update|重启更新)$/i}).first();
  await ready.waitFor({state:'visible',timeout:420000});
  const approved='ac001c0358942e474c72413e4cbf76c86a6635e0e3b5f8b0e3d85722726e5714';
  const pending=await digest(path.join(profile,'updates','pending_update.bin'));
  assert.equal(pending,approved);
  const pendingState=await json(path.join(profile,'updates','pending_update.json'));
  assert.equal(pendingState.version,'0.6.1');
  assert.equal(await digest(exe),originalHash);
  await first.page.screenshot({path:path.join(evidence,'update-ready.png')});
  event('original 0.6.0 downloaded the published 0.6.1 installer',pending);
  await ready.click({timeout:15000});
  event('clicked the rendered Restart update button','0.6.0');
  const confirm=first.page.getByRole('button',{name:/^(exit|quit|退出)$/i}).last();
  if(await confirm.waitFor({timeout:5000}).then(()=>true,()=>false)) {
    await confirm.click({timeout:5000}).catch(()=>{});
  }
  await poll('0.6.0 exits for the Windows installer',()=>original.exitCode!==null,110000);
  const updatedHash='fd5e65f252fd93a6f5dff829cf9ade81faa9eaac40508c1c9fccbc3302030d8b';
  await poll('application binary replaced by the released updater',async()=>await digest(exe)===updatedHash,180000);
  const latest=await connect('0.6.1');
  event('automatic relaunch','0.6.1');
  const after=await snapshot(latest.page,'after-061');
  assert.deepEqual(after,before);

  const logs=path.join(profile,'logs');
  const fromApp = await poll('native updater public URL and install log',async()=>{
    const parts=await Promise.all((await fsp.readdir(logs))
      .filter(name=>/^unified-.*\.log$/.test(name))
      .map(name=>fsp.readFile(path.join(logs,name),'utf8')));
    const lines=parts.join('\n').split(/\r?\n/);
    const start=lines.find(line=>line.includes('Checking for updates... Current: v0.6.0, Target: windows-x86_64') &&
      line.includes('Endpoint: "https://github.com/micsnq2yoma-lgtm/AgenTeam/releases/latest/download/latest.json"'));
    const recheck=lines.find(line=>line.includes('Network check succeeded on attempt') && line.includes('(server v0.6.1)'));
    const install=lines.find(line=>line.includes('Installing v0.6.1...'));
    return start && recheck && install ? {start,recheck,install} : null;
  },15000);
  await write(path.join(evidence,'original-updater-events.json'),JSON.stringify(fromApp,null,2));
  event('public upgrade and retention passed',{installerSHA256:pending,installedApplicationSHA256:updatedHash});
}
main().then(()=>process.exit(0),err=>{console.error(err.stack||err);process.exit(1);});
