(() => {
  'use strict';

  const storeKey = 'sever.design.workflow.v3';
  const types = {
    start: { title: 'Старт', icon: 'play' },
    agent: { title: 'Действие', icon: 'bot' },
    gate: { title: 'Условие', icon: 'branch' },
    end: { title: 'Финиш', icon: 'finish' }
  };
  const paths = {
    play: 'M7 4l12 8-12 8z',
    bot: 'M5 7h14a2 2 0 0 1 2 2v9H3V9a2 2 0 0 1 2-2z M12 3v4 M8 12v2 M16 12v2 M8 21h8',
    branch: 'M12 2l10 10-10 10L2 12z M8 12h8 M12 8v8',
    finish: 'M5 21V3m0 1c5-3 9 3 15 0v10c-6 3-10-3-15 0',
    hand: 'M7 12V6a1.5 1.5 0 0 1 3 0v5-7a1.5 1.5 0 0 1 3 0v7-6a1.5 1.5 0 0 1 3 0v7-4a1.5 1.5 0 0 1 3 0v7c0 4-2 7-6 7-3 0-5-2-7-5l-3-4a1.5 1.5 0 0 1 2-2z',
    pointer: 'M5 3l14 9-7 1-3 7z',
    plus: 'M12 5v14 M5 12h14', minus: 'M5 12h14',
    fit: 'M9 4H4v5 M15 4h5v5 M20 15v5h-5 M4 15v5h5',
    link: 'M9 15l6-6 M8 16l-1 1a3 3 0 0 1-4-4l4-4a3 3 0 0 1 4 0 M13 15a3 3 0 0 0 4 0l4-4a3 3 0 0 0-4-4l-1 1',
    close: 'M6 6l12 12 M18 6L6 18', back: 'M3 10l5-5 M3 10h10a7 7 0 0 1 0 14', redo: 'M21 9l-5-5 M21 9h-9a6 6 0 0 0 0 12',
    controls: 'M3 6h7 M14 6h7 M3 12h12 M19 12h2 M3 18h4 M11 18h10 M10 3v6 M15 9v6 M7 15v6',
    page: 'M5 3h10l4 4v14H5z M14 3v5h5 M8 12h8 M8 16h5',
    check: 'M4 12l5 5L20 6', user: 'M12 4a4 4 0 1 0 .1 0 M4 21v-2q0-5 8-5t8 5v2',
    chevron:'M8 4l8 8-8 8', bin:'M4 6h16 M9 6V3h6v3 M6 6l1 15h10l1-15 M10 10v7 M14 10v7',
    grid:'M3 3h6v6H3z M15 3h6v6h-6z M3 15h6v6H3z M15 15h6v6h-6z',
    eye:'M2 12q10-15 20 0-10 15-20 0z M12 9a3 3 0 1 0 .1 0',
    addStep:'M4 4h11v10H4z M17 13v8 M13 17h8',
    folder:'M3 6h7l2 2h9v11H3z'
  };
  const icon = (name, size=18) => '<svg width="'+size+'" height="'+size+'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="'+paths[name]+'"/></svg>';
  const esc = value => String(value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const seed = {
    nodes: [
      {id:'start',kind:'start',name:'Приглашение',role:'Владелец',input:'Новый запрос',output:'Приглашение создано',x:0,y:185},
      {id:'prepare',kind:'agent',name:'Подготовить доступ',role:'Аналитик A-02',input:'Приглашение',output:'Политика доступа',x:200,y:153},
      {id:'review',kind:'gate',name:'Доступ согласован?',role:'Владелец',input:'Политика доступа',output:'Решение владельца',x:487,y:153},
      {id:'grant',kind:'agent',name:'Открыть раздел',role:'Оркестратор O-01',input:'Решение владельца',output:'Доступ к разделу',x:786,y:14},
      {id:'revise',kind:'agent',name:'Уточнить права',role:'Аналитик A-02',input:'Замечания владельца',output:'Уточнённая редакция',x:786,y:319},
      {id:'finish',kind:'end',name:'Доступ выдан',role:'Оркестратор O-01',input:'Доступ к разделу',output:'Процесс завершён',x:1050,y:46}
    ],
    links: [
      {id:'l1',from:'start',to:'prepare',label:''},{id:'l2',from:'prepare',to:'review',label:''},
      {id:'l3',from:'review',to:'grant',label:'Да'},{id:'l4',from:'review',to:'revise',label:'Нет'},
      {id:'l5',from:'grant',to:'finish',label:''},{id:'l6',from:'revise',to:'prepare',label:'Повторная проверка'}
    ]
  };
  const clone = data => JSON.parse(JSON.stringify(data));
  const dims = node => node.kind==='start'||node.kind==='end' ? {w:160,h:54} : {w:210,h:118};
  const clamp = (value,lo,hi) => Math.max(lo,Math.min(hi,value));
  let volatileDraft = null;
  function readDraft(){
    if(volatileDraft)return clone(volatileDraft);
    try {
      const data=JSON.parse(localStorage.getItem(storeKey));
      if(!data || !Array.isArray(data.nodes)||!Array.isArray(data.links))return clone(seed);
      const ids=new Set();
      for(const n of data.nodes){
        if(!Object.hasOwn(types,n.kind)||!Number.isFinite(n.x)||!Number.isFinite(n.y)||
          !['id','name','role','input','output'].every(k=>typeof n[k]==='string')||ids.has(n.id))return clone(seed);
        ids.add(n.id);
      }
      if(!data.links.every(l=>ids.has(l.from)&&ids.has(l.to)&&typeof l.label==='string'&&typeof l.id==='string'))return clone(seed);
      return data;
    }catch{return clone(seed);}
  }
  function contents(data){
    if(!data.nodes.length)return {x:0,y:0,w:650,h:310};
    const minX=Math.min(...data.nodes.map(n=>n.x));
    const minY=Math.min(...data.nodes.map(n=>n.y));
    const maxX=Math.max(...data.nodes.map(n=>n.x+dims(n).w));
    const maxY=Math.max(...data.nodes.map(n=>n.y+dims(n).h));
    return {x:minX-29,y:minY-33,w:maxX-minX+58,h:maxY-minY+107};
  }
  function rounded(points){
    const out=['M'+points[0].join(' ')];
    for(let i=1;i<points.length-1;i++){
      const a=points[i-1], b=points[i], c=points[i+1];
      const la=Math.hypot(b[0]-a[0],b[1]-a[1]),lc=Math.hypot(c[0]-b[0],c[1]-b[1]);
      if(!la||!lc)continue;
      const r=Math.min(18,la/2,lc/2);
      const before=b.map((v,k)=>v+(a[k]-v)*r/la), after=b.map((v,k)=>v+(c[k]-v)*r/lc);
      out.push('L'+before.join(' ')+' Q'+b.join(' ')+' '+after.join(' '));
    }
    return out.join(' ')+' L'+points.at(-1).join(' ');
  }
  function routeEdge(a,b){
    const ad=dims(a), bd=dims(b), x=a.x+ad.w+3, y=a.y+ad.h/2, tx=b.x-6, ty=b.y+bd.h/2;
    if(tx>=x+28){
      const mid=(x+tx)/2;
      return {d:rounded([[x,y],[mid,y],[mid,ty],[tx,ty]]),lx:mid,ly:Math.abs(y-ty)>45?(y+ty)/2:ty-16};
    }
    const low=Math.max(a.y+ad.h,b.y+bd.h)+40;
    return {d:rounded([[x,y],[x+26,y],[x+26,low],[tx-28,low],[tx-28,ty],[tx,ty]]),lx:(x+tx)/2,ly:low};
  }
  function mount(host,ctx){
    let data=readDraft(), selected=data.nodes.find(n=>n.id==='review')?.id||data.nodes[0]?.id;
    let selectedEdge=null, inspect=false, tool='pointer', pending=null, gesture=null, ignoreClick=false;
    let addMenu=false, storageOK=!volatileDraft, scale=1, dx=0, dy=0, previousSize=null;
    const undo=[], redo=[];
    const root=document.createElement('section');root.className='wf-root';root.setAttribute('aria-label','Конструктор процессов');
    host.replaceChildren(root);
    const btn=(a,name,ico,extra='')=>'<button type="button" data-wf="'+a+'" title="'+esc(name)+'" aria-label="'+esc(name)+'" '+extra+'>'+icon(ico)+'</button>';
    root.innerHTML='<div class="wf-stage" tabindex="0" role="region" aria-label="Холст процесса. Перетаскивайте фон для перемещения. Клавиши плюс и минус меняют масштаб."><div class="wf-world"><svg class="wf-wires" aria-hidden="true"></svg><div class="wf-nodes"></div></div></div>'+
      '<header class="wf-heading"><div class="wf-breadcrumb">'+icon('folder',15)+' Север <span>/</span> Онбординг <span>/</span> Процессы</div><h1>Приём участника</h1><div class="wf-heading-meta"><span class="wf-draft"><i></i> Черновик</span><span class="wf-count"></span></div></header>'+
      '<div class="wf-sideactions"><button type="button" data-wf="inspectToggle">'+icon('controls',16)+'<span>Свойства</span></button></div>'+
      '<aside class="wf-inspector" aria-label="Свойства элемента" hidden></aside>'+
      '<div class="wf-dock"><div class="wf-tools" role="toolbar" aria-label="Инструменты конструктора">'+
      btn('pointer','Выбрать и двигать шаги','pointer')+btn('hand','Перемещать холст','hand')+'<span class="wf-sep"></span>'+
      btn('add','Добавить элемент','plus','aria-haspopup="true"')+btn('connect','Соединить шаги','link')+'<span class="wf-sep"></span>'+
      btn('undo','Отменить изменение','back')+btn('redo','Повторить изменение','redo')+'</div><div class="wf-add" hidden></div></div>'+
      '<div class="wf-bottom"><span class="wf-storage"></span><div class="wf-zoom">'+btn('zoomOut','Уменьшить масштаб','minus')+
      '<output class="wf-scale"></output>'+btn('zoomIn','Увеличить масштаб','plus')+'<i></i>'+btn('fit','Показать весь процесс','fit')+'</div></div>'+
      '<div class="wf-hint" role="status" aria-live="polite"></div>';
    const q=s=>root.querySelector(s), stage=q('.wf-stage'), world=q('.wf-world'), panel=q('.wf-inspector');
    const source=id=>data.nodes.find(n=>n.id===id);
    const current=()=>source(selected);
    const wire=()=>data.links.find(l=>l.id===selectedEdge);
    const hint=s=>{q('.wf-hint').textContent=s;};
    function save(){
      try {localStorage.setItem(storeKey,JSON.stringify(data));volatileDraft=null;storageOK=true;}
      catch{volatileDraft=clone(data);storageOK=false;}
      status();
    }
    function status(){
      q('.wf-storage').innerHTML=icon(storageOK?'check':'eye',13)+(storageOK?'Локальный черновик':'Изменения только в окне');
    }
    function remember(){
      undo.push(clone(data)); if(undo.length>48)undo.shift(); redo.length=0;
    }
    function restore(from,to){
      if(!from.length)return;
      to.push(clone(data));data=from.pop();
      if(!current())selected=data.nodes[0]?.id;
      if(!wire())selectedEdge=null;
      pending=null;tool='pointer';save();draw();
    }
    function transform(){
      world.style.transform='translate('+dx+'px,'+dy+'px) scale('+scale+')';
      root.style.setProperty('--wf-dot',(20*scale)+'px');
      stage.style.backgroundPosition=dx+'px '+dy+'px';
      q('.wf-scale').value=Math.round(scale*100)+'%';
    }
    function available(){
      const w=stage.clientWidth,h=stage.clientHeight,r=root.getBoundingClientRect();
      const top=Math.min(h*.39,q('.wf-heading').getBoundingClientRect().bottom-r.top+14);
      const right=inspect&&w>780?300:0;
      return {x:15,y:top,w:Math.max(120,w-right-30),h:Math.max(140,h-top-90)};
    }
    function fit(initial=false){
      const bounds=contents(data), area=available();
      scale=clamp(Math.min(area.w/bounds.w,area.h/bounds.h),.15,1.55);
      dx=area.x+(area.w-bounds.w*scale)/2-bounds.x*scale;
      dy=area.y+(area.h-bounds.h*scale)/2-bounds.y*scale;
      if(initial&&stage.clientWidth<640&&current()){
        scale=Math.max(.76,scale);
        const n=current(),size=dims(n);
        dx=area.x+area.w/2-(n.x+size.w/2)*scale;
        dy=area.y+area.h/2-(n.y+size.h/2)*scale;
      }
      transform();
    }
    function setScale(next,x,y){
      next=clamp(next,.15,2.2);
      dx=x-(x-dx)*next/scale;dy=y-(y-dy)*next/scale;scale=next;transform();
    }
    function zoom(factor){const a=available();setScale(scale*factor,a.x+a.w/2,a.y+a.h/2);}
    function renderLinks(){
      const markup=data.links.map(l=>{
        const a=source(l.from),b=source(l.to),edge=routeEdge(a,b);
        const tone=l.id===selectedEdge?'selected':a.kind==='gate'?(l.label==='Нет'?'no':'yes'):'basic';
        const attr='data-wf-link="'+esc(l.id)+'"';
        const width=Math.max(31,l.label.length*6+18);
        return '<g class="wf-wire wf-wire-'+tone+'" '+attr+'><path d="'+edge.d+'" class="wf-wline" marker-end="url(#wf-tip-'+tone+')"/><path d="'+edge.d+'" class="wf-hit"/>'+
          (l.label?'<g transform="translate('+edge.lx+' '+edge.ly+')"><rect x="'+(-width/2)+'" y="-10" width="'+width+'" height="20" rx="9"/><text y="4" text-anchor="middle">'+esc(l.label)+'</text></g>':'')+'</g>';
      }).join('');
      const markers=['basic','selected','yes','no'].map(t=>'<marker id="wf-tip-'+t+'" viewBox="0 0 8 8" refX="6" refY="4" markerWidth="5" markerHeight="5" orient="auto"><path class="wf-tip-'+t+'" d="M1 1 L7 4 L1 7z"/></marker>').join('');
      q('.wf-wires').innerHTML='<defs>'+markers+'</defs>'+markup;
    }
    function nodeMarkup(n){
      const t=types[n.kind],small=n.kind==='start'||n.kind==='end',d=dims(n);
      return '<div class="wf-node wf-'+n.kind+(n.id===selected?' wf-chosen':'')+(pending===n.id?' wf-pending':'')+'" data-node="'+esc(n.id)+'" style="left:'+n.x+'px;top:'+n.y+'px;width:'+d.w+'px;height:'+d.h+'px">'+
        (n.kind!=='start'?'<button type="button" data-port="in" aria-label="Вход в '+esc(n.name)+'" class="wf-port wf-port-in"></button>':'')+
        '<button class="wf-face" type="button" aria-label="'+esc(t.title+' — '+n.name)+'"><span class="wf-type"><i>'+icon(t.icon,small?17:15)+'</i>'+(!small?t.title:'')+'</span><strong>'+esc(n.name)+'</strong>'+
        (small?'':'<span class="wf-role">'+icon(n.kind==='gate'?'user':'bot',13)+esc(n.role||'Без исполнителя')+'</span>')+'</button>'+
        (n.kind!=='end'?'<button type="button" data-port="out" aria-label="Провести связь из '+esc(n.name)+'" class="wf-port wf-port-out"></button>':'')+'</div>';
    }
    function renderNodes(){q('.wf-nodes').innerHTML=data.nodes.map(nodeMarkup).join('');}
    const field=(label,key,value,limit)=>'<label class="wf-field"><span>'+label+'</span><input name="'+key+'" maxlength="'+limit+'" value="'+esc(value)+'"></label>';
    function inspector(){
      panel.hidden=!inspect;root.classList.toggle('wf-inspecting',inspect);
      if(!inspect)return;
      const n=current(),l=wire();
      const title=l?'Переход':n?types[n.kind].title:'Процесс';
      const head='<div class="wf-panel-head"><span>'+title+'</span>'+btn('inspectClose','Закрыть свойства','close')+'</div>';
      if(l){
        panel.innerHTML=head+'<h2>Связь шагов</h2><p class="wf-context">'+esc(source(l.from).name)+' → '+esc(source(l.to).name)+'</p>'+
        '<form data-form="link">'+field('Подпись перехода','label',l.label,36)+'<button class="wf-apply">Сохранить</button></form>'+
        '<button class="wf-remove" data-wf="removeLink">'+icon('bin',15)+' Удалить переход</button>';return;
      }
      if(!n){panel.innerHTML=head+'<h2>Свойства процесса</h2><p>Выберите шаг на холсте или добавьте новый элемент.</p>';return;}
      const related=data.links.filter(e=>e.from===n.id||e.to===n.id);
      const isTask=n.kind==='gate'||n.kind==='agent';
      panel.innerHTML=head+'<h2>'+esc(n.name)+'</h2><form data-form="node">'+field(n.kind==='gate'?'Вопрос условия':'Название','name',n.name,64)+
        (isTask?field('Исполнитель','role',n.role,60):'')+field('Вход','input',n.input,90)+field('Результат','output',n.output,90)+
        '<button class="wf-apply">Сохранить</button></form>'+
        '<details class="wf-connections"><summary>Переходы <b>'+related.length+'</b></summary>'+related.map(e=>'<button data-edge-id="'+esc(e.id)+'" type="button">'+esc(source(e.from).name)+' → '+esc(source(e.to).name)+'</button>').join('')+'</details>'+
        (n.id==='prepare'||n.id==='review'?'<button class="wf-doc" data-wf="source">'+icon('page',15)+' Политика доступа '+icon('chevron',12)+'</button>':'')+
        '<button class="wf-remove" data-wf="removeNode">'+icon('bin',15)+' Удалить шаг</button>';
    }
    function toolbar(){
      for(const t of ['pointer','hand','connect'])q('[data-wf="'+t+'"]').setAttribute('aria-pressed',String(tool===t));
      q('[data-wf="inspectToggle"]').setAttribute('aria-expanded',String(inspect));
      q('[data-wf="undo"]').disabled=!undo.length;q('[data-wf="redo"]').disabled=!redo.length;
      q('[data-wf="add"]').setAttribute('aria-expanded',String(addMenu));
      const menu=q('.wf-add');menu.hidden=!addMenu;
      if(addMenu)menu.innerHTML='<strong>Добавить элемент</strong>'+Object.entries(types).map(([kind,t])=>'<button type="button" data-wf="new-'+kind+'" class="wf-menu-'+kind+'">'+icon(t.icon,17)+t.title+'</button>').join('');
      stage.dataset.tool=tool;
    }
    function draw(){
      renderNodes();renderLinks();inspector();toolbar();transform();status();
      const n=data.nodes.length,k=n%10,y=n%100;
      const form=y>=11&&y<=14?'элементов':k===1?'элемент':k>=2&&k<=4?'элемента':'элементов';
      q('.wf-count').textContent=n+' '+form;
    }
    function selectNode(id){
      selected=id;selectedEdge=null;inspect=true;addMenu=false;hint('');draw();
    }
    function selectLink(id){
      selected=null;selectedEdge=id;inspect=true;addMenu=false;pending=null;tool='pointer';hint('');draw();
    }
    function setTool(value){
      pending=null;tool=value;addMenu=false;
      hint(value==='connect'?'Выберите исходный шаг, затем конечный.':'');draw();
    }
    function connect(id,port){
      const node=source(id);
      if(!pending){
        if(node.kind==='end'||port==='in'){hint('Начните с выходного порта другого шага.');return;}
        pending=id;tool='connect';hint('Выберите вход следующего шага.');draw();return;
      }
      if(pending===id||node.kind==='start'||port==='out'){hint('Выберите вход другого шага.');return;}
      if(data.links.some(l=>l.from===pending&&l.to===id)){hint('Эти шаги уже соединены.');return;}
      remember();const edge={id:uid(),from:pending,to:id,label:''};data.links.push(edge);
      selected=null;selectedEdge=edge.id;pending=null;tool='pointer';inspect=true;save();draw();hint('Переход добавлен.');
    }
    function uid(){return globalThis.crypto?.randomUUID?.()||Date.now().toString(36)+'-'+Math.random().toString(36).slice(2);}
    function addNode(kind){
      const a=available(), d=dims({kind});const x=(a.x+a.w*.5-dx)/scale-d.w/2,y=(a.y+a.h*.55-dy)/scale-d.h/2;
      let nx=x,ny=y;
      while(data.nodes.some(n=>Math.abs(n.x-nx)<110&&Math.abs(n.y-ny)<65)){nx+=37;ny+=37;}
      const n={id:uid(),kind,name:({agent:'Новое действие',gate:'Новое условие',start:'Начало',end:'Завершение'})[kind],role:'',input:'',output:'',x:Math.round(nx),y:Math.round(ny)};
      remember();data.nodes.push(n);pending=null;tool='pointer';selectNode(n.id);save();panel.querySelector('[name="name"]').select();
    }
    function remove(){
      if(!current()&&!wire())return;
      remember();
      if(wire())data.links=data.links.filter(l=>l.id!==selectedEdge);
      else {data.nodes=data.nodes.filter(n=>n.id!==selected);data.links=data.links.filter(l=>l.from!==selected&&l.to!==selected);}
      selected=null;selectedEdge=null;inspect=false;pending=null;hint('Удалено. Изменение можно отменить.');save();draw();stage.focus();
    }
    function handleClick(e){
      if(ignoreClick){ignoreClick=false;return;}
      const a=e.target.closest('[data-wf]')?.dataset.wf;
      const edge=e.target.closest('[data-wf-link],[data-edge-id]');
      if(edge&&!a){selectLink(edge.dataset.wfLink||edge.dataset.edgeId);return;}
      const n=e.target.closest('[data-node]');
      if(n&&!a){
        const port=e.target.closest('[data-port]')?.dataset.port;
        if(port||tool==='connect')connect(n.dataset.node,port);else selectNode(n.dataset.node);
        return;
      }
      if(!a){if(addMenu){addMenu=false;toolbar();}return;}
      if(a.startsWith('new-')){addNode(a.slice(4));return;}
      switch(a){
        case 'pointer':case 'hand':case 'connect':setTool(tool==='connect'&&a==='connect'?'pointer':a);break;
        case 'zoomIn':zoom(1.2);break;
        case 'zoomOut':zoom(1/1.2);break;
        case 'fit':fit();break;
        case 'add':addMenu=!addMenu;toolbar();if(addMenu)q('.wf-add button').focus();break;
        case 'undo':restore(undo,redo);break;
        case 'redo':restore(redo,undo);break;
        case 'inspectToggle':inspect=!inspect;inspector();toolbar();break;
        case 'inspectClose':inspect=false;inspector();toolbar();q('[data-wf="inspectToggle"]').focus();break;
        case 'removeNode':case 'removeLink':remove();break;
        case 'source':ctx.go('doc','access');break;
      }
    }
    function submit(e){
      const form=e.target.closest('form[data-form]');if(!form)return;e.preventDefault();
      const record=form.dataset.form==='link'?wire():current();if(!record)return;
      const changes=Object.fromEntries([...new FormData(form)].map(([key,value])=>[key,value.trim()]));
      if('name' in changes&&!changes.name){form.elements.name.focus();hint('Укажите название шага.');return;}
      if(Object.entries(changes).some(([key,value])=>record[key]!==value)){
        remember();Object.assign(record,changes);save();draw();hint('Свойства сохранены.');
      }
    }
    function down(e){
      if(e.button!==0||e.target.closest('[data-port],[data-wf-link]')||tool==='connect')return;
      const target=e.target.closest('[data-node]'),node=target&&tool!=='hand'?source(target.dataset.node):null;
      if(gesture)return;
      gesture={id:e.pointerId,node,x:e.clientX,y:e.clientY,ox:node?node.x:dx,oy:node?node.y:dy,moved:false};
    }
    function move(e){
      if(!gesture||gesture.id!==e.pointerId)return;
      const g=gesture,x=e.clientX-g.x,y=e.clientY-g.y;
      if(!g.moved&&Math.hypot(x,y)<4)return;
      if(!g.moved){g.moved=true;stage.setPointerCapture(e.pointerId);if(g.node)remember();}
      if(g.node){
        g.node.x=Math.round(g.ox+x/scale);g.node.y=Math.round(g.oy+y/scale);
        const el=Array.from(q('.wf-nodes').children).find(item=>item.dataset.node===g.node.id);
        if(el){el.style.left=g.node.x+'px';el.style.top=g.node.y+'px';}
        renderLinks();
      }else{dx=g.ox+x;dy=g.oy+y;transform();}
    }
    function finish(e){
      if(!gesture||gesture.id!==e.pointerId)return;
      if(gesture.moved){ignoreClick=e.type==='pointerup';if(gesture.node){save();toolbar();}}
      gesture=null;
      if(stage.hasPointerCapture(e.pointerId))stage.releasePointerCapture(e.pointerId);
    }
    function wheel(e){
      e.preventDefault();
      if(e.ctrlKey||e.metaKey){
        const r=stage.getBoundingClientRect();
        setScale(scale*Math.exp(-clamp(e.deltaY,-100,100)*.012),e.clientX-r.left,e.clientY-r.top);
      }else{dx-=e.deltaX;dy-=e.deltaY;transform();}
    }
    function keys(e){
      if(e.target.tagName==='INPUT'||e.target.tagName==='TEXTAREA')return;
      if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='z'){e.preventDefault();restore(e.shiftKey?redo:undo,e.shiftKey?undo:redo);return;}
      if(e.key==='Escape'){pending=null;tool='pointer';addMenu=false;inspect=false;hint('');draw();stage.focus();return;}
      if(!stage.contains(e.target))return;
      if(e.key==='+'||e.key==='='){e.preventDefault();zoom(1.2);return;}
      if(e.key==='-'){e.preventDefault();zoom(1/1.2);return;}
      const arrows={ArrowLeft:[-1,0],ArrowRight:[1,0],ArrowUp:[0,-1],ArrowDown:[0,1]};
      if(!Object.hasOwn(arrows,e.key))return;
      const target=e.target.closest('[data-node]'),node=target&&source(target.dataset.node);
      const [x,y]=arrows[e.key];e.preventDefault();
      if(node){remember();const unit=e.shiftKey?40:10;node.x+=x*unit;node.y+=y*unit;save();draw();
        Array.from(q('.wf-nodes').children).find(el=>el.dataset.node===node.id)?.querySelector('.wf-face').focus();
      }else{dx-=x*44;dy-=y*44;transform();}
    }
    const events=new AbortController(), on=(el,name,fn,options={})=>el.addEventListener(name,fn,{...options,signal:events.signal});
    on(root,'pointerdown',()=>{ignoreClick=false;});on(root,'click',handleClick);on(root,'submit',submit);on(root,'keydown',keys);
    on(stage,'pointerdown',down);on(window,'pointermove',move);on(window,'pointerup',finish);on(window,'pointercancel',finish);
    on(stage,'wheel',wheel,{passive:false});
    draw();fit(true);
    const obs=new ResizeObserver(()=>{
      const next={w:stage.clientWidth,h:stage.clientHeight};
      if(previousSize&&(next.w!==previousSize.w||next.h!==previousSize.h))fit(true);
      previousSize=next;
    });
    obs.observe(stage);
    return ()=>{events.abort();obs.disconnect();root.remove();};
  }
  window.KBWorkflow={mount};
})();
