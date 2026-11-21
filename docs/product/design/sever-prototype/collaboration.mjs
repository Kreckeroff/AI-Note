const changeKeys = ['settings','profiles','spaceDetails','comments','revisions','coverImages','approach'];
const equal = (left,right) => JSON.stringify(left) === JSON.stringify(right);

function mergeRecords(base,working,saved,mergeValue = (_,after) => after) {
  const merged = {...saved};
  for (const key of new Set([...Object.keys(base),...Object.keys(working)])) {
    if (equal(base[key],working[key])) continue;
    if (Object.hasOwn(working,key)) merged[key] = mergeValue(base[key],working[key],saved[key]);
    else delete merged[key];
  }
  return merged;
}

function mergeMembers(base = {},working = {},saved = {}) {
  return mergeRecords(base,working,saved);
}
function mergePagePermissions(base = {},working,saved = {}) {
  return {...mergeRecords(base,working,saved),members:mergeMembers(base.members,working.members,saved.members)};
}
export function mergeWorkspaceChanges(base,working,saved) {
  const incorporated = {...working};
  for (const key of changeKeys) {
    incorporated[key] = mergeRecords(base[key],working[key],saved[key]);
  }
  incorporated.memberships = mergeRecords(base.memberships,working.memberships,saved.memberships,mergeMembers);
  incorporated.permissions = mergeRecords(base.permissions,working.permissions,saved.permissions,mergePagePermissions);
  const records = collection => Object.fromEntries(collection.map(item => [item.id,item]));
  for (const key of ['documents','tasks']) {
    incorporated[key] = Object.values(mergeRecords(records(base[key]),records(working[key]),records(saved[key])));
  }
  return incorporated;
}

function editSpan(base,changed) {
  let start = 0, end = base.length, changedEnd = changed.length;
  while (start < end && start < changedEnd && base[start] === changed[start]) start++;
  while (end > start && changedEnd > start && base[end-1] === changed[changedEnd-1]) {end--; changedEnd--;}
  return {start,end,text:changed.slice(start,changedEnd)};
}

function mergeBody(base,draft,saved) {
  const left = editSpan(base,draft), right = editSpan(base,saved);
  if (left.start === right.start || (left.end > right.start && right.end > left.start)) return undefined;
  const [first,second] = [left,right].sort((a,b) => a.start-b.start);
  return base.slice(0,first.start)+first.text+base.slice(first.end,second.start)+second.text+base.slice(second.end);
}

export function mergePageDraft(base,draft,saved) {
  const fields = {};
  for (const key of ['title','body','cover']) {
    if (draft[key] === base[key] || draft[key] === saved[key]) fields[key] = saved[key];
    else if (saved[key] === base[key]) fields[key] = draft[key];
    else if (key === 'body') fields.body = mergeBody(base.body,draft.body,saved.body);
    if (fields[key] === undefined) return undefined;
  }
  return fields;
}

export function withWorkspaceLock(save,scope = '') {
  return navigator.locks ? navigator.locks.request('sever.workspace.writes' + scope,save) : Promise.resolve().then(save);
}

export function connectPresence(onChange,scope = '') {
  const id = crypto.randomUUID();
  const channel = typeof BroadcastChannel === 'function' ? new BroadcastChannel('sever.workspace.presence.v1' + scope) : null;
  const peers = new Map();
  const heartbeat = 2000, lifetime = 6800;
  let current = null;
  const send = (type, state = current) => channel?.postMessage({id,type,state});
  const refresh = () => onChange(current ? [current,...[...peers.values()].map(({state})=>state).filter(state=>state.page===current.page)] : []);
  if (channel) channel.onmessage = ({data}) => {
    const {id:peerId,type,state} = data || {};
    if (!peerId || peerId === id) return;
    const previous = peers.get(peerId);
    if (type === 'leave') {
      if (peers.delete(peerId)) refresh();
    } else if ((type === 'hello' || type === 'heartbeat') && state?.page && state.account) {
      peers.set(peerId,{state,lastSeen:Date.now()});
      if (!previous || !equal(previous.state,state)) refresh();
      if (type === 'hello' && current) send('heartbeat');
    }
  };
  setInterval(() => {
    let removed = false;
    for (const [peer,entry] of peers) if (Date.now()-entry.lastSeen > lifetime) {
      peers.delete(peer); removed = true;
    }
    if (removed) refresh();
    if (current) send('heartbeat');
  },heartbeat);
  window.addEventListener('pagehide',() => { if (current) send('leave',null); });
  window.addEventListener('pageshow',() => { if (current) send('hello'); });
  return state => {
    if (equal(current,state)) {refresh(); return;}
    current = state;
    send(current ? 'hello' : 'leave',current);
    refresh();
  };
}
