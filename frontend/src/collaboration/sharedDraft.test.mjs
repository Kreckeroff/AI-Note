import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {registerHooks} from 'node:module';
import ts from 'typescript';
import test from 'node:test';

const hooks = registerHooks({
  resolve(specifier,context,next) {
    return next(/^(?:\.\.?\/)[\w/]+$/u.test(specifier) ? specifier+'.ts' : specifier,context);
  },
  load(url,context,next) {
    if (url.endsWith('.ts')) return {shortCircuit:true,format:'module',
      source:ts.transpileModule(readFileSync(new URL(url),'utf8'),{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText};
    return next(url,context);
  },
});
const {SharedDraftSession}=await import('./sharedDraft.ts');
const {createDraftHistory}=await import('../editor/draftHistory.ts');
const {rebaseSharedHistory}=await import('./sharedDraftHistory.ts');
const {
  applySplice, applySplices, carrySplice, orderedSplices, projectPrivateMedia, liftPrivateMediaWithSteps,
  projectPrivateMediaSplice, spliceAtSelection,
}=await import('./draftSplice.ts');
hooks.deregister();

const lease='11111111-1111-4111-a111-111111111111';
const patch=(start,end,text)=>({start,end,text});
const draft=source=>({source,format:'markdown'});
const state=(version,source,extra={})=>({...draft(source),version:String(version),publishedRevision:'1',lease,...extra});

function session(t,preserved,onDraft) {
  const timers=new Map();
  const requests=[];
  const shown=[];
  const views=[];
  let stamp=0;
  const original={fetch:globalThis.fetch,window:globalThis.window};
  globalThis.window={
    setTimeout(callback) {timers.set(++stamp,callback);return stamp;},
    clearTimeout(id) {timers.delete(id);},
    addEventListener() {},
    removeEventListener() {},
  };
  globalThis.fetch=(href,options)=>new Promise(resolve=>requests.push({
    href,body:options.body?JSON.parse(options.body):null,resolve,
  }));
  const transport=new SharedDraftSession('/coedit',{
    onDraft:(text,steps)=>{shown.push({text,steps});onDraft?.(text,steps);},
    onState:view=>views.push(view),
    onDenied:()=>assert.fail('Unexpected denial'),
  },preserved);
  t.after(()=>{
    transport.stop();
    Object.assign(globalThis,original);
  });
  transport.start();
  function send() {
    const item=timers.entries().next().value;
    assert.ok(item,'Scheduled operation');
    timers.delete(item[0]);
    item[1]();
    assert.equal(requests.length,1);
    return requests.shift();
  }
  async function reply(request,value,status=200) {
    request.resolve(Response.json(value,{status}));
    await new Promise(resolve=>setImmediate(resolve));
  }
  return {send,reply,shown,views,transport};
}

test('typing during a poll keeps the authored base and disjoint ordered steps keep the queued tail',async t=>{
  const {send,reply,shown,transport}=session(t);
  await reply(send(),state(0,'abcdef'));
  const poll=send();
  transport.edit(draft('abcWdef'),patch(3,3,'W'));
  transport.edit(draft('abcWdVef'),patch(5,5,'V'));
  const remote=[patch(0,0,'L'),patch(7,7,'R')];
  await reply(poll,state(2,'LabcdefR',{fromVersion:'0',steps:remote}));
  const first=send();
  assert.equal(first.body.version,'0');
  assert.equal(first.body.start,3);
  const applied=patch(4,4,'W');
  await reply(first,state(3,'LabcWdefR',{fromVersion:'0',steps:[...remote,applied],applied,acknowledgedVersion:'3'}));
  const tail=send();
  assert.equal(tail.body.version,'3');
  assert.equal(tail.body.start,6);
  assert.equal(shown.at(-1).text.source,'LabcWdVefR');
});

test('a rolled-out acknowledged operation updates both the guarded snapshot and the next poll',async t=>{
  const {send,reply,views,transport}=session(t);
  await reply(send(),state(0,'abcd'));
  transport.edit(draft('abcdQ'),patch(4,4,'Q'));
  const write=send();
  await reply(write,{code:'baseExpired',...state(300,'Текущий Q'),acknowledgedVersion:'1'},409);
  assert.equal(views.at(-1).status,'ready');
  assert.equal(views.at(-1).pending,0);
  assert.equal(views.at(-1).snapshot.version,'300');
  assert.equal(new URL(send().href,'http://localhost').searchParams.get('after'),'300');
});

test('a changed base preserves an unacknowledged command separately',async t=>{
  const {send,reply,views,transport}=session(t);
  await reply(send(),state(0,'abcd'));
  transport.edit(draft('abcdQ'),patch(4,4,'Q'));
  await reply(send(),{code:'baseExpired',...state(300,'Текущий')},409);
  assert.equal(views.at(-1).status,'recover');
  assert.equal(views.at(-1).recovery,'abcdQ');
  assert.equal(views.at(-1).snapshot.version,'300');
});

test('equal resulting bytes still shift a queued splice through an exact ordered journal',async t=>{
  const {send,reply,shown,transport}=session(t);
  await reply(send(),state(0,'aaaaZ'));
  transport.edit(draft('aaaaZH'),patch(5,5,'H'));
  transport.edit(draft('aaXaZH'),patch(2,3,'X'));
  const applied=patch(5,5,'H');
  await reply(send(),state(3,'aaaaZH',{fromVersion:'0',
    steps:[patch(0,1,''),patch(3,3,'a'),applied],applied,acknowledgedVersion:'3'}));
  const tail=send();
  assert.equal(tail.body.start,1);
  assert.equal(tail.body.end,2);
  assert.equal(shown.at(-1).text.source,'aXaaZH');
});

test('a peer resolution keeps the locally edited merge fragment after removing the conflict',async t=>{
  const {send,reply,views,transport}=session(t);
  await reply(send(),{code:'conflict',...state(1,'abcd'),persistent:true,start:1,end:3,mine:'XY',theirs:'bc'},409);
  transport.changeMergeInput('Моё совмещение');
  assert.equal(views.at(-1).mergeInput,'Моё совмещение');
  assert.ok(transport.hasUnconfirmed());
  await reply(send(),state(2,'aVd',{fromVersion:'1',steps:[patch(1,3,'V')]}));
  assert.equal(views.at(-1).status,'ready');
  assert.equal(views.at(-1).copy,'Моё совмещение');
  assert.ok(transport.hasUnconfirmed());
});

test('open and resolve a persistent conflict without losing the held local copy',async t=>{
  const {send,reply,shown,views,transport}=session(t,draft('Мой ожидающий текст'));
  await reply(send(),{code:'conflict',...state(3,'Текущий'),persistent:true,start:0,end:7,mine:'Команда',theirs:'Текущий'},409);
  assert.equal(shown.at(-1).text.source,'Текущий');
  transport.resolve('Общий');
  const resolution=send();
  assert.equal(resolution.body.resolve,true);
  await reply(resolution,state(4,'Общий',{acknowledgedVersion:'4'}));
  assert.equal(views.at(-1).status,'recover');
  assert.equal(views.at(-1).recovery,'Мой ожидающий текст');
  transport.useCurrent();
  assert.equal(views.at(-1).copy,'Мой ожидающий текст');
  assert.equal(shown.at(-1).text.source,'Общий');
});

test('a disjoint edit keeps the persisted conflict and its genuine acknowledged text',async t=>{
  const {send,reply,shown,views,transport}=session(t);
  const conflict={start:2,end:4,mine:'XY',theirs:'cd'};
  await reply(send(),{code:'conflict',...state(2,'abcdef'),persistent:true,...conflict},409);
  assert.equal(transport.edit(draft('abXYef'),patch(2,4,'XY')),false);
  assert.equal(transport.edit(draft('Qabcdef'),patch(0,0,'Q')),true);
  const write=send();
  assert.equal(write.body.version,'2');
  const applied=patch(0,0,'Q');
  await reply(write,state(3,'Qabcdef',{fromVersion:'2',steps:[applied],applied,acknowledgedVersion:'3',
    restoreConflict:{...conflict,start:3,end:5}}));
  assert.equal(shown.at(-1).text.source,'Qabcdef');
  assert.equal(views.at(-1).conflict.start,3);
  assert.equal(views.at(-1).canResolve,true);
  transport.resolve('XY');
  const resolution=send();
  assert.equal(resolution.body.start,3);
  assert.equal(resolution.body.resolve,true);
  await reply(resolution,state(4,'QabXYef',{acknowledgedVersion:'4'}));
  assert.equal(views.at(-1).status,'ready');
});

test('manual replacement supersedes only the rejected command and retains later input',async t=>{
  const {send,reply,views,transport}=session(t);
  await reply(send(),state(0,'abcdef'));
  transport.edit(draft('abXdef'),patch(2,3,'X'));
  transport.edit(draft('abXdefQ'),patch(6,6,'Q'));
  await reply(send(),{code:'conflict',...state(1,'abYdef'),persistent:false,
    start:2,end:3,mine:'X',theirs:'Y'},409);
  transport.resolve('Z');
  await reply(send(),state(2,'abZdef',{acknowledgedVersion:'2'}));
  assert.equal(views.at(-1).status,'recover');
  assert.equal(views.at(-1).recovery,'abXdefQ');
});

test('outside typing during a transient conflict follows the exact resolved fragment',async t=>{
  const {send,reply,shown,views,transport}=session(t);
  await reply(send(),state(0,'abcd'));
  transport.edit(draft('aXcd'),patch(1,2,'X'));
  await reply(send(),{code:'conflict',...state(1,'aYcd'),persistent:false,start:1,end:2,mine:'X',theirs:'Y'},409);
  assert.equal(transport.edit(draft('aZcd'),patch(1,2,'Z')),false);
  assert.equal(transport.edit(draft('aYcdQ'),patch(4,4,'Q')),true);
  transport.resolve('Z');
  const resolution=send();
  const applied=patch(1,2,'Z');
  assert.equal(resolution.body.text,'Z');
  await reply(resolution,state(2,'aZcd',{fromVersion:'1',steps:[applied],applied,acknowledgedVersion:'2'}));
  assert.equal(shown.at(-1).text.source,'aZcdQ');
  assert.equal(views.at(-1).recovery,undefined);
  const append=send();
  assert.deepEqual([append.body.version,append.body.start,append.body.text],['2',4,'Q']);
  const appended=patch(4,4,'Q');
  await reply(append,state(3,'aZcdQ',{fromVersion:'2',steps:[appended],applied:appended,acknowledgedVersion:'3'}));
  assert.equal(views.at(-1).status,'ready');
  assert.equal(views.at(-1).pending,0);
});

function editorSession(t, staged=[]) {
  const history=createDraftHistory();
  const frame=(source,start=0)=>({source,contentFormat:'markdown',selection:{start,end:start}});
  let view=frame('');
  const project=({source})=>projectPrivateMedia(draft(source),staged);
  const active=session(t,undefined,(text,steps)=>{
    const lifted=liftPrivateMediaWithSteps(project(view),text,staged,steps);
    assert.ok(lifted,'Remote edit preserves staged placement');
    if (lifted.steps?.length) rebaseSharedHistory(history,view.source,lifted.steps);
    view=frame(lifted.text.source);
  });
  function edit(splice) {
    const next=frame(applySplice(view.source,splice),splice.start+splice.text.length);
    const projected=project(next).text;
    const exact=projectPrivateMediaSplice(project(view),projected,splice);
    assert.ok(exact,'Local author geometry projects to shared text');
    assert.equal(active.transport.edit(projected,exact),true);
    history.record(view,next,'atomic',0,splice);
    view=next;
  }
  function undo() {
    const undone=history.step('undo',(from,to,span,blocked)=>{
      assert.equal(blocked,undefined,'An independent edit does not block Undo');
      assert.equal(from.source,view.source,'History starts at the live optimistic source');
      assert.ok(span,'The authored visual splice is retained');
      const projected=project(to).text;
      const exact=projectPrivateMediaSplice(project(view),projected,span);
      assert.ok(exact);
      assert.equal(active.transport.edit(projected,exact),true);
      return to;
    });
    assert.ok(undone);
    view=undone;
  }
  return {...active,edit,undo,stage:source=>{view=frame(source);},source:()=>view.source};
}

test('the second editor can undo its same-gap insertion after the first editor wins the gap',async t=>{
  const {send,reply,edit,undo,source}=editorSession(t);
  await reply(send(),state(0,'text'));
  edit(patch(0,0,'B '));
  const applied=patch(2,2,'B ');
  await reply(send(),state(2,'A B text',{fromVersion:'0',steps:[patch(0,0,'A '),applied],applied,acknowledgedVersion:'2'}));
  assert.equal(source(),'A B text');
  undo();
  assert.equal(source(),'A text');
  const inverse=send();
  assert.deepEqual([inverse.body.start,inverse.body.end,inverse.body.text],[2,4,'']);
});

test('ordinary history keeps a peer prefix and suffix when undoing a concurrent replacement',async t=>{
  const {send,reply,edit,undo,source}=editorSession(t);
  await reply(send(),state(0,'abcdef'));
  edit(patch(2,4,'X'));
  const applied=patch(3,5,'X');
  await reply(send(),state(3,'LabXefR',{fromVersion:'0',
    steps:[patch(0,0,'L'),patch(7,7,'R'),applied],applied,acknowledgedVersion:'3'}));
  undo();
  assert.equal(source(),'LabcdefR');
  const inverse=send();
  assert.deepEqual([inverse.body.start,inverse.body.end,inverse.body.text],[3,4,'cd']);
});

test('a FIFO ACK rebases the real history through pending local and ordered disjoint remote edits',async t=>{
  const {send,reply,edit,undo,source}=editorSession(t);
  await reply(send(),state(0,'abcdef'));
  const poll=send();
  edit(patch(3,3,'W'));
  edit(patch(5,5,'V'));
  const remote=[patch(0,0,'L'),patch(7,7,'R')];
  await reply(poll,state(2,'LabcdefR',{fromVersion:'0',steps:remote}));
  const first=send();
  assert.deepEqual([first.body.version,first.body.start],['0',3]);
  const head=patch(4,4,'W');
  await reply(first,state(3,'LabcWdefR',{fromVersion:'0',steps:[...remote,head],applied:head,acknowledgedVersion:'3'}));
  const tail=send();
  assert.deepEqual([tail.body.version,tail.body.start],['3',6]);
  const second=patch(6,6,'V');
  await reply(tail,state(4,'LabcWdVefR',{fromVersion:'3',steps:[second],applied:second,acknowledgedVersion:'4'}));
  undo();
  assert.equal(source(),'LabcWdefR');
  undo();
  assert.equal(source(),'LabcdefR');
});

test('a FIFO ACK preserves the intended copy and its Undo through a net-zero remote journal',async t=>{
  const {send,reply,edit,undo,source}=editorSession(t);
  await reply(send(),state(0,'aaaaZ'));
  edit(patch(5,5,'H'));
  edit(patch(2,3,'X'));
  const head=patch(5,5,'H');
  await reply(send(),state(3,'aaaaZH',{fromVersion:'0',
    steps:[patch(0,1,''),patch(3,3,'a'),head],applied:head,acknowledgedVersion:'3'}));
  const tail=send();
  assert.deepEqual([tail.body.start,tail.body.end],[1,2]);
  const second=patch(1,2,'X');
  await reply(tail,state(4,'aXaaZH',{fromVersion:'3',steps:[second],applied:second,acknowledgedVersion:'4'}));
  undo();
  assert.equal(source(),'aaaaZH');
  const inverse=send();
  assert.deepEqual([inverse.body.start,inverse.body.end,inverse.body.text],[1,2,'a']);
});

test('Undo keeps its visual span at the shared gap next to a staged image',async t=>{
  const image=`![](attachment:${lease})`;
  const {send,reply,edit,undo,stage,source}=editorSession(t,[lease]);
  await reply(send(),state(0,'r:aaaa'));
  stage('r:'+image+'aaaa');
  edit(patch(2,2,'a'));
  const own=send();
  assert.deepEqual([own.body.start,own.body.end],[2,2]);
  const applied=patch(2,2,'a');
  await reply(own,state(1,'r:aaaaa',{fromVersion:'0',steps:[applied],applied,acknowledgedVersion:'1'}));
  await reply(send(),state(2,'r:azaaa',{fromVersion:'1',steps:[patch(3,4,'z')]}));
  assert.equal(source(),'r:a'+image+'zaaa');
  undo();
  assert.equal(source(),'r:'+image+'zaaa');
  const inverse=send();
  assert.deepEqual([inverse.body.start,inverse.body.end,inverse.body.text],[2,3,'']);
});

test('undo of the first accepted insertion uses the ordered remote offset despite repeated prefixes',()=>{
  const original='Текст из тестовых данных.';
  const mine='Вставка Марии. ';
  const peers='Вставка Елены. ';
  const current=mine+original;
  const remote=orderedSplices(current,[patch(mine.length,mine.length,peers)]);
  const inverse=patch(0,mine.length,'');
  const rebased=carrySplice(inverse,remote.splices);
  assert.ok(rebased);
  assert.equal(applySplice(remote.source,rebased.local),peers+original);
  assert.equal(applySplices(original,rebased.carried),peers+original);
});

test('exact caret position disambiguates insertion among identical characters',()=>{
  assert.deepEqual(spliceAtSelection('aaa','aaaa',patch(1,1,''),patch(2,2,'')),patch(1,1,'a'));
});

test('a private reference stays out of shared text and relocation waits at a crossing replacement',()=>{
  const id='11111111-1111-4111-a111-111111111111';
  const media=`![](attachment:${id})`;
  const original=projectPrivateMedia(draft('До '+media+' после'),[id]);
  assert.equal(original.text.source,'До  после');
  assert.equal(liftPrivateMediaWithSteps(original,draft('Другое'),[id],[patch(0,9,'Другое')]),undefined);
  const next=liftPrivateMediaWithSteps(original,draft('Сначала До  после'),[id],[patch(0,0,'Сначала ')]);
  assert.equal(next.text.source,'Сначала До '+media+' после');
});

test('an ordered net-zero shared journal moves the private anchor and lifts exact source offsets',()=>{
  const id='11111111-1111-4111-a111-111111111111';
  const media=`![](attachment:${id})`;
  const original='aa'+media+'aaZ';
  const projection=projectPrivateMedia(draft(original),[id]);
  const lifted=liftPrivateMediaWithSteps(projection,draft('aaaaZ'),[id],[patch(0,1,''),patch(3,3,'a')]);
  assert.ok(lifted);
  assert.equal(lifted.text.source,'a'+media+'aaaZ');
  assert.equal(applySplices(original,lifted.steps),lifted.text.source);
});
