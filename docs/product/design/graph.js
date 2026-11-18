/* Spatial knowledge atlas. The same world positions are used at every point in history. */
(() => {
  'use strict';

  const kinds = {document: 'Документ', fact: 'Знание', work: 'Задача', agent: 'Исполнитель', record: 'Запись'};

  // Key, title, family, source page, creation day, world coordinates, relative visual weight.
  const records = [
    ['access','Доступ к пространству','document','access',0,0,0,0,1],
    ['space','Пространство Север','document','access',.25,-190,-68,-53,.65],
    ['directory','Раздел онбординга','document','access',.6,-247,-104,23,.46],
    ['revision','Исходная редакция','record','access',1.05,-120,65,-85,.36],
    ['template','Шаблон приглашения','document','access',1.45,-118,-132,40,.64],
    ['viewer','Просмотр раздела','fact','scope',2.05,142,-71,-54,.50],
    ['scope','Граница раздела','fact','scope',2.55,144,-114,28,.78],
    ['editor','Редактирование','fact','scope',3,232,-45,-33,.43],
    ['grant','Приглашение','record','access',3.55,-229,43,41,.52],
    ['invitation','Вход по приглашению','work','access',4.05,-158,81,81,.78],
    ['decision','Решение владельца','record','handoff',4.5,34,138,20,.60],
    ['audit','Журнал доступа','document','access',5,250,82,-64,.59],
    ['agent','Аналитик A-02','agent','handoff',5.4,-95,140,-7,.65],
    ['policy','Правило приглашений','fact','scope',5.9,257,-125,-67,.48],
    ['check','Проверка доступа','fact','scope',6.65,175,40,94,.70],
    ['handoff','Хэндофф аналитика','document','handoff',7.15,-64,107,115,.73],
    ['owner','Владелец пространства','agent','handoff',7.6,29,175,-43,.50],
    ['reader','Роль читателя','fact','scope',8.1,94,-139,-54,.40],
    ['team','Рабочая группа','record','access',8.35,-282,-23,-52,.34],
    ['section','Соседний раздел','document','scope',8.65,273,-63,38,.41],
    ['request','Запрос расширения','record','scope',9,237,57,64,.40],
    ['review','Согласование роли','record','handoff',9.3,132,138,53,.43],
    ['orchestrator','Оркестратор O-01','agent','handoff',9.7,-108,200,-55,.50],
    ['protocol','Протокол решения','document','handoff',10,57,203,-92,.47],
    ['usage','Отметка использования','record','handoff',10.35,194,121,-47,.34],
    ['evidence','Проверенный источник','fact','scope',10.75,57,-168,17,.48]
  ];
  const nodes = records.map(([key,name,family,source,born,x,y,z,weight]) => ({key,name,family,source,born,x,y,z,weight}));
  const byKey = new Map(nodes.map(n => [n.key,n]));
  const descriptions = {
    access:'Определяет, кто может пригласить участника, какой раздел он увидит и кто согласует изменение роли.',
    space:'Объединяет документы и процессы команды «Север». Внутри пространства действуют правила доступа к разделам.',
    directory:'Хранит страницы, по которым новый участник знакомится со своим рабочим пространством.',
    revision:'Первая сохранённая редакция политики доступа к пространству.',
    template:'Задаёт состав приглашения, область доступа и место для результата.',
    viewer:'Даёт участнику возможность читать документы выбранного раздела.',
    scope:'Приглашение открывает только указанный раздел. Для соседнего раздела потребуется отдельное разрешение.',
    editor:'Разрешает предлагать изменения материалов в пределах выбранного раздела.',
    grant:'Связывает получателя, пространство и разрешённое действие.',
    invitation:'Аналитик подготовил условия приглашения. Владелец проверяет редакцию перед согласованием.',
    decision:'Фиксирует, кто утвердил роль, для какого раздела и на основании какой редакции.',
    audit:'Собирает записи об изменениях прав, исполнителях и времени изменений.',
    agent:'Аналитик проверяет границы раздела и готовит материал для владельца пространства.',
    policy:'Уточняет допустимые действия с приглашением в этой области.',
    check:'Перед выдачей контекста агент сверяет пространство, раздел и запрошенное действие.',
    handoff:'Передаёт владельцу редакцию политики, связанную задачу и основания для согласования.',
    owner:'Принимает решение об изменении доступа на основании переданных материалов.',
    reader:'Описывает набор действий участника с правом просмотра.',
    team:'Участники пространства, которым разрешена работа с материалами онбординга.',
    section:'Материалы соседнего раздела имеют собственную границу доступа.',
    request:'Сохраняет запрос на расширение прав и ожидаемое действие.',
    review:'Проверяет соответствие выбранной роли решению владельца пространства.',
    orchestrator:'Передаёт согласованный результат следующему участнику процесса.',
    protocol:'Хранит решение владельца и версию документа, на которой оно основано.',
    usage:'Показывает, какой источник использовал агент, когда передавал задачу.',
    evidence:'Связывает принятое утверждение с исходной редакцией политики.'
  };
  const relations = [
    ['access','space'],['space','directory'],['access','revision'],['access','template'],['template','directory'],
    ['access','viewer'],['access','scope'],['scope','viewer'],['scope','editor'],['template','grant'],
    ['access','invitation'],['grant','invitation'],['access','decision'],['invitation','decision'],['decision','audit'],
    ['invitation','agent'],['revision','audit'],['policy','scope'],['grant','policy'],['check','scope'],
    ['check','viewer'],['check','access'],['handoff','agent'],['handoff','access'],['handoff','invitation'],
    ['owner','decision'],['owner','handoff'],['reader','viewer'],['reader','policy'],['space','team'],
    ['team','directory'],['section','scope'],['request','check'],['request','section'],['review','request'],
    ['review','owner'],['review','decision'],['orchestrator','handoff'],['orchestrator','agent'],
    ['protocol','decision'],['protocol','audit'],['usage','audit'],['usage','handoff'],['usage','protocol'],
    ['evidence','scope'],['evidence','access'],['evidence','revision']
  ];
  const timelineEnd = 11;
  const events = [
    [0,'Создана политика доступа'],[.6,'Добавлен раздел онбординга'],[1.45,'Сохранён шаблон приглашения'],
    [2.55,'Подтверждена граница раздела'],[3.55,'Сформировано приглашение'],[4.05,'Связана задача KB-014'],
    [5,'Открыт журнал доступа'],[5.9,'Уточнены правила приглашений'],[6.65,'Предложен навык проверки'],
    [7.15,'Подготовлен хэндофф'],[8.1,'Добавлена роль читателя'],[9,'Запрошено расширение прав'],
    [10,'Сохранён протокол решения'],[10.75,'Подтверждён источник']
  ];
  const iconPaths = {
    theme:'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18z M12 3a9 9 0 0 1 0 18z',
    play:'M8 4.5 20 12 8 19.5z',pause:'M8 5v14m8-14v14', rewind:'M5 5v14m14-14L8 12l11 7z',
    search:'M16 16l5 5M18 10.5a7.5 7.5 0 1 1-15 0 7.5 7.5 0 0 1 15 0z',
    close:'m6 6 12 12m0-12L6 18',panel:'M3 4h18v16H3z M14 4v16',arrow:'M4 12h15m-6-6 6 6-6 6',
    doc:'M6 3h8l4 4v14H6z M14 3v5h5 M9 12h6m-6 4h6', fit:'M3 9V3h6m6 0h6v6m0 6v6h-6m-6 0H3v-6',
    plus:'M5 12h14m-7-7v14',minus:'M5 12h14',orbit:'M21 12a9 9 0 1 1-9-9m4 0h5v5m0-5-7 7',
    network:'M4 16 11 5l9 10-11 3z',hand:'m7 12-2-2c-2-2-4 0-2 2l7 8h7l4-9c.5-2-2-3-3-1l-1 1V5c0-2-3-2-3 0v4-6c0-2-3-2-3 0v6-4c0-2-3-2-3 0v7z'
  };
  function icon(name,size=16) {
    return '<svg aria-hidden="true" width="'+size+'" height="'+size+'" viewBox="0 0 24 24" fill="'+(name==='play'||name==='theme'?'currentColor':'none')+'" stroke="currentColor" stroke-width="'+(name==='pause'?3.4:1.6)+'" stroke-linejoin="round" stroke-linecap="round"><path fill-rule="evenodd" d="'+iconPaths[name]+'"/></svg>';
  }
  function control(action,label,glyph){
    return '<button type="button" class="kg-tool" data-kg="'+action+'" aria-label="'+label+'" title="'+label+'">'+icon(glyph)+'</button>';
  }
  function clamp(x,min,max){return Math.max(min,Math.min(max,x));}
  function ease(t){const v=clamp(t,0,1);return v*v*(3-2*v);}
  function rgb(color,alpha){return 'rgba('+color.join(',')+','+alpha+')';}
  function lerp(a,b,t){return a+(b-a)*t;}
  function stamp(day){return String(7+Math.floor(day+.0001)).padStart(2,'0')+' ноя';}
  function eventAt(day){let title=events[0][1];for(const e of events)if(e[0]<=day)title=e[1];return title;}
  function tag(n){return n.key==='check'?'Навык':n.key==='scope'?'Факт':kinds[n.family];}
  function stateLabel(node){
    if(node.key==='access')return 'Редакция 03';
    if(node.key==='scope'||node.key==='evidence')return 'Подтверждено';
    if(node.key==='check')return 'Предложено агентом';
    return node.family==='work'?'KB-014':node.family==='document'?'Редакция 01':'Онбординг';
  }
  function accessibleName(node){return tag(node)+' · '+node.name;}

  const preferenceKey = 'sever.graph.theme';
  function initialTheme(){
    const explicit = new URL(location.href).searchParams.get('theme');
    if(explicit === 'dark' || explicit === 'light')return explicit;
    try{return localStorage.getItem(preferenceKey) === 'light' ? 'light' : 'dark';}
    catch{return 'dark';}
  }
  function readPalette(el){
    const css = getComputedStyle(el);
    const token = name => css.getPropertyValue('--kg-'+name).trim();
    const colors = Object.fromEntries(Object.keys(kinds).map(family => {
      const hex = token(family).slice(1);
      return [family, [0,2,4].map(i => parseInt(hex.slice(i,i+2),16))];
    }));
    return {colors, grid:token('grid'), backdrop:token('bg'), ink:token('ink'), secondary:token('secondary')};
  }
  function dot(family){return 'style="--dot:var(--kg-'+family+')"';}

  function renderSphere(p,ctx,color,selected,phase,light,opacity){
    const {x,y,r} = p;
    ctx.save();
    ctx.globalAlpha = opacity;
    ctx.translate(x,y);
    ctx.scale(r,r);
    function haze(cx,cy,rx,ry,stops){
      ctx.save();
      ctx.translate(cx,cy);
      ctx.scale(rx,ry);
      const gradient = ctx.createRadialGradient(0,0,0,0,0,1);
      for(const [stop,value] of stops)gradient.addColorStop(stop,value);
      ctx.fillStyle = gradient;
      ctx.fillRect(-1,-1,2,2);
      ctx.restore();
    }
    const tint = (toward,amount) => rgb(color.map(channel=>Math.round(lerp(channel,toward,amount))),1);
    const radius = selected ? 2.45 : 1.6;
    haze(0,0,radius,radius,[[0,rgb(color,selected?.35:.18)],[.52,rgb(color,selected?.12:.05)],[1,rgb(color,0)]]);
    ctx.beginPath();
    ctx.arc(0,0,1,0,Math.PI*2);
    ctx.clip();
    const volume = ctx.createRadialGradient(-.25,-.3,.02,.02,.14,1.13);
    volume.addColorStop(0,tint(255,light?.50:.63));
    volume.addColorStop(.54,rgb(color,1));
    volume.addColorStop(.86,tint(28,light?.26:.51));
    volume.addColorStop(1,rgb(color,.74));
    ctx.fillStyle = volume;
    ctx.fillRect(-1,-1,2,2);
    // Broad clouds remain inside the surface. Only the selected node moves during playback.
    const drift = selected ? Math.sin(phase)*.12 : 0;
    ctx.rotate(-.48+drift*.6);
    haze(-.12+drift,-.36,.91,.48,[[0,'rgba(251,251,255,.49)'],[.45,'rgba(235,239,255,.20)'],[1,'rgba(235,239,255,0)']]);
    if(selected || r>8){
      haze(.40,-.02+drift,.58,.52,[[0,rgb(color,.79)],[1,rgb(color,0)]]);
      haze(.10-drift,.34,.96,.45,[[0,'rgba(225,240,255,.35)'],[.48,'rgba(224,235,254,.16)'],[1,'rgba(224,235,254,0)']]);
      haze(-.5,.13, .58,.41,[[0,rgb(color,.57)],[1,rgb(color,0)]]);
    }
    if(selected){
      const cloud=ctx.createLinearGradient(-1,-.4,.94,.65);
      cloud.addColorStop(0,'rgba(224,232,255,0)');
      cloud.addColorStop(.29,'rgba(217,228,255,.17)');
      cloud.addColorStop(.52,'rgba(245,244,255,.66)');
      cloud.addColorStop(.77,'rgba(225,239,255,.30)');
      cloud.addColorStop(1,'rgba(225,239,255,0)');
      ctx.fillStyle=cloud;
      ctx.beginPath();
      ctx.moveTo(-1.08,-.39);
      ctx.bezierCurveTo(-.36,-1.08,.07,-.63+drift,.11,-.13+drift);
      ctx.bezierCurveTo(.18,.44,.67,.61,1.06,-.04);
      ctx.bezierCurveTo(.72,.94,.02,.62,-.13,.05+drift);
      ctx.bezierCurveTo(-.27,-.42,-.49,-.69,-1.08,-.39);
      ctx.fill();
    }
    ctx.restore();
  }
  function mount(host,api){
    let theme=initialTheme();
    document.body.dataset.graphTheme=theme;
    const available=nodes.filter(n=>api.boards!==false||n.family!=='work');
    const availableKeys=new Set(available.map(n=>n.key));
    const edges=relations.filter(([a,b])=>availableKeys.has(a)&&availableKeys.has(b)).map(([a,b])=>[byKey.get(a),byKey.get(b)]);
    const nodeFor=key=>availableKeys.has(key)?byKey.get(key):byKey.get('access');
    let selected=nodeFor(api.key),time=timelineEnd,playing=false,speed=1;
    let yaw=-.12,pitch=-.08,zoom=1,frame=0,lastTime=0,disposed=false,drag=null,hover=null,spots=[];
    let width=1,height=1,pan={x:selected.x,y:selected.y,z:selected.z},cameraTarget=null;
    const abort=new AbortController(), on=(el,type,fn,options={})=>el.addEventListener(type,fn,{...options,signal:abort.signal});
    const reduced=matchMedia('(prefers-reduced-motion: reduce)');
    const compact=()=>host.clientWidth<=760;
    let inspectorOpen=!compact();
    host.innerHTML='<section class="kg-scene" aria-label="Граф знаний"><canvas class="kg-field" tabindex="0" aria-label="Пространственная карта. Стрелки вращают, плюс и минус меняют масштаб. Для выбора узла откройте поиск."></canvas>'+
      '<header class="kg-heading"><div class="kg-crumb">Север <span>/</span> Онбординг</div><h1>Граф знаний</h1><div class="kg-census"></div></header>'+
      '<div class="kg-tools">'+control('search','Найти узел','search')+control('panel','Сведения об узле','panel')+control('theme','Светлая тема','theme')+'<i></i>'+control('fit','Исходный ракурс','fit')+control('plus','Приблизить','plus')+control('minus','Отдалить','minus')+'</div>'+
      '<aside class="kg-inspect" aria-label="Выбранный узел"></aside>'+
      '<div class="kg-legend"><span><i style="--dot:var(--kg-document)"></i>Документы</span><span><i style="--dot:var(--kg-fact)"></i>Знания</span><span><i style="--dot:var(--kg-work)"></i>Задачи</span><span><i style="--dot:var(--kg-agent)"></i>Исполнители</span></div>'+
      '<section class="kg-transport" aria-label="История графа"><button type="button" class="kg-play" data-kg="play" aria-label="Воспроизвести историю">'+icon('play',20)+'</button><div class="kg-film"><div class="kg-film-top"><strong>История графа</strong><span class="kg-event"></span><time></time></div><label class="kg-track"><span class="kg-hidden">Момент истории</span><span class="kg-trace" aria-hidden="true"></span><input type="range" min="0" max="11" step=".01" value="11" aria-label="Момент истории"></label><div class="kg-dates"><span>07 ноя</span><span>12 ноя</span><span>18 ноя</span></div></div><button type="button" class="kg-speed" data-kg="speed" aria-label="Скорость воспроизведения">1×</button></section>'+
      '<dialog class="kg-find"><div class="kg-find-title"><strong>Найти узел</strong>'+control('close-search','Закрыть поиск','close')+'</div><input type="search" aria-label="Название узла" placeholder="Название документа, навыка или задачи"><div class="kg-results"></div></dialog></section>';
    const root=host.querySelector('.kg-scene'), $=s=>root.querySelector(s);
    let palette=readPalette(root);
    const canvas=$('.kg-field'),g=canvas.getContext('2d'),inspector=$('.kg-inspect'),range=$('.kg-track input'),finder=$('.kg-find');
    const controls=$('.kg-tools'), transport=$('.kg-transport'),searchInput=finder.querySelector('input');
    if(api.boards===false)$('.kg-legend span:nth-child(3)').remove();
    const trace=$('.kg-trace');
    trace.innerHTML=Array.from({length:90},(_,i)=>{
      const density=available.reduce((sum,n)=>sum+Math.exp(-Math.abs(i/89*11-n.born)*4),0);
      return '<i style="height:'+Math.min(99,14+density*22)+'%"></i>';
    }).join('');
    function updateThemeButton(){
      const button=controls.querySelector('[data-kg="theme"]');
      button.setAttribute('aria-pressed',String(theme==='light'));
      button.title=theme==='dark'?'Включить светлую тему':'Включить тёмную тему';
    }
    function toggleTheme(){
      theme=theme==='dark'?'light':'dark';
      document.body.dataset.graphTheme=theme;
      palette=readPalette(root);
      updateThemeButton();
      try{localStorage.setItem(preferenceKey,theme);}catch{/* Private storage may be unavailable. */}
      const url=new URL(location.href);
      url.searchParams.set('theme',theme);
      history.replaceState(history.state,'',url);
      api.onThemeChange?.(theme);
      schedule();
    }
    function neighbours(n){return edges.filter(([a,b])=>a===n||b===n).map(([a,b])=>a===n?b:a).filter(x=>x.born<=time);}
    function updateInspector(){
      root.classList.toggle('kg-inspecting',inspectorOpen);
      inspector.hidden=!inspectorOpen;
      controls.querySelector('[data-kg="panel"]').setAttribute('aria-expanded',String(inspectorOpen));
      if(!inspectorOpen)return;
      const peers=neighbours(selected);
      const task=selected.key==='invitation' && api.taskInfo('KB-014');
      inspector.innerHTML='<div class="kg-inspect-head"><span class="kg-kind" '+dot(selected.family)+'>'+tag(selected)+'</span>'+control('panel','Скрыть сведения','close')+'</div><div class="kg-inspect-body"><h2>'+selected.name+'</h2><div class="kg-version">'+stateLabel(selected)+'</div><p>'+descriptions[selected.key]+'</p><div class="kg-source"><small>Источник</small><button type="button" data-kg="source">'+icon('doc')+'<span data-kg-source-title></span>'+icon('arrow',14)+'</button></div>'+
        (task?'<button type="button" class="kg-task" data-kg="task" data-task-id="'+task.id+'">Открыть задачу '+task.id+' '+icon('arrow',14)+'</button>':'')+
        (peers.length?'<div class="kg-related"><small>Связанные узлы <b>'+peers.length+'</b></small>'+peers.map(n=>'<button type="button" data-key="'+n.key+'"><i '+dot(n.family)+'></i>'+n.name+icon('arrow',13)+'</button>').join('')+'</div>':'')+'</div>';
      inspector.querySelector('[data-kg-source-title]').textContent=api.sourceTitle(task ? task.sourceKey : selected.source);
    }
    let lastCount=-1,lastPeerCount=-1;
    function updateHistory(){
      const count=available.filter(n=>n.born<=time).length;
      const links=edges.filter(([a,b])=>Math.max(a.born,b.born)<=time).length;
      const peerCount=neighbours(selected).length;
      if(count!==lastCount){$('.kg-census').textContent=count+' '+plural(count,['узел','узла','узлов'])+' · '+links+' '+plural(links,['связь','связи','связей']);lastCount=count;}
      if(peerCount!==lastPeerCount){updateInspector();lastPeerCount=peerCount;}
      range.value=time;range.setAttribute('aria-valuetext',stamp(time)+' · '+eventAt(time));trace.style.setProperty('--progress',(time/11*100)+'%');
      $('.kg-event').textContent=eventAt(time);$('.kg-film-top time').textContent=stamp(time);
    }
    function plural(n,forms){const mod=n%100;return forms[mod>=11&&mod<=14?2:n%10===1?0:n%10>=2&&n%10<=4?1:2];}
    function updatePlay(){
      const button=$('.kg-play');
      button.innerHTML=icon(playing?'pause':'play',20);
      button.setAttribute('aria-label',playing?'Приостановить историю':'Воспроизвести историю');
      button.title=button.getAttribute('aria-label');
      root.classList.toggle('kg-playing',playing);
    }
    function schedule(){if(!frame&&!disposed)frame=requestAnimationFrame(tick);}
    function cameraTo(n){
      cameraTarget={x:n.x,y:n.y,z:n.z};
      if(reduced.matches){pan=cameraTarget;cameraTarget=null;}
      schedule();
    }
    function choose(n,show=true){
      if(!availableKeys.has(n.key))return;
      selected=n;if(show)inspectorOpen=true;
      api.navigateKey?.(n.key);cameraTo(n);lastPeerCount=-1;updateInspector();
      if(finder.open)finder.close();
    }
    function setTime(t){
      time=clamp(t,0,11);
      if(selected.born>time)choose(byKey.get('access'),false);
      updateHistory();schedule();
    }
    function play(){
      if(playing){playing=false;}
      else {if(time>=11-.005){setTime(0);yaw=-.24;pitch=-.1;}playing=true;lastTime=0;}
      updatePlay();schedule();
    }
    function tick(now){
      frame=0;if(disposed)return;
      const dt=lastTime?Math.min((now-lastTime)/1000,.08):0;lastTime=now;
      if(playing){
        time=Math.min(11,time+dt*.67*speed);
        if(!reduced.matches)yaw+=dt*.026;
        updateHistory();
        if(time>=11){playing=false;updatePlay();}
      }
      if(cameraTarget){
        const f=1-Math.exp(-Math.max(dt,.016)*8);
        pan={x:lerp(pan.x,cameraTarget.x,f),y:lerp(pan.y,cameraTarget.y,f),z:lerp(pan.z,cameraTarget.z,f)};
        if(Math.hypot(pan.x-cameraTarget.x,pan.y-cameraTarget.y,pan.z-cameraTarget.z)<.25){pan=cameraTarget;cameraTarget=null;}
      }
      draw();if(playing||cameraTarget)schedule();
    }
    function viewport(){
      const side=inspectorOpen&&!compact()?Math.min(width*.255,320)+26:0;
      const top=compact()?128:125, bottom=compact()?120:118;
      const areaWidth=width-side, areaHeight=Math.max(120,height-top-bottom);
      return {ox:areaWidth*.52,oy:top+areaHeight*.51,scale:Math.max(.31,Math.min(areaWidth/625,areaHeight/385))*zoom};
    }
    function projector(){
      const {ox,oy,scale}=viewport();
      const cy=Math.cos(yaw),sy=Math.sin(yaw),cp=Math.cos(pitch),sp=Math.sin(pitch);
      return n=>{
        const dx=n.x-pan.x,dy=n.y-pan.y,dz=n.z-pan.z;
        const x=dx*cy+dz*sy,rz=-dx*sy+dz*cy;
        const y=dy*cp-rz*sp,z=dy*sp+rz*cp;
        const f=640/Math.max(175,640-z);
        return {x:ox+x*f*scale,y:oy+y*f*scale,z,f};
      };
    }
    function revealProgress(born,duration){
      if(reduced.matches || time===timelineEnd)return 1;
      return ease((time-born)/Math.min(duration,timelineEnd-born));
    }
    function draw(){
      if(!g)return;
      const project=projector(),area=viewport(),colors=palette.colors,light=theme==='light';
      g.clearRect(0,0,width,height);
      // Very fine projected reference grid makes depth visible when orbiting.
      g.save();g.strokeStyle=palette.grid;g.lineWidth=.72;
      for(let k=-8;k<=8;k++){
        for(const axis of [0,1]){
          const a=project({x:axis?-620:k*55,y:154,z:axis?k*55:-440});
          const b=project({x:axis?620:k*55,y:154,z:axis?k*55:385});
          g.beginPath();g.moveTo(a.x,a.y);g.lineTo(b.x,b.y);g.stroke();
        }
      }g.restore();
      const projected=new Map(),near=new Set(neighbours(selected));
      spots=[];
      for(const n of available){
        if(n.born>time)continue;
        const progress=n.born===0?1:revealProgress(n.born,.52);
        const p=project(n);
        p.r=(n===selected?24:4+n.weight*6.8)*clamp(area.scale,.68,1.55)*p.f*(.25+.75*progress);
        p.node=n;p.alpha=progress;spots.push(p);projected.set(n,p);
      }
      for(const [a,b] of edges){
        const p=projected.get(a),q=projected.get(b);if(!p||!q)continue;
        const active=a===selected||b===selected;
        const birth=Math.max(a.born,b.born),reveal=revealProgress(birth,.78);
        if(reveal<=0)continue;
        const mx=(p.x+q.x)/2+(p.y-q.y)*.09,my=(p.y+q.y)/2+(q.x-p.x)*.09;
        const cx=lerp(p.x,mx,reveal),cy=lerp(p.y,my,reveal);
        const ex=lerp(cx,lerp(mx,q.x,reveal),reveal),ey=lerp(cy,lerp(my,q.y,reveal),reveal);
        g.beginPath();g.moveTo(p.x,p.y);g.quadraticCurveTo(cx,cy,ex,ey);
        g.lineWidth=active?1.35:.92;
        const grad=g.createLinearGradient(p.x,p.y,q.x+.01,q.y+.01);
        grad.addColorStop(0,rgb(colors[a.family],active?.80:light?.42:.32));grad.addColorStop(1,rgb(colors[b.family],active?.88:light?.46:.36));
        g.strokeStyle=grad;g.stroke();
        if(active){
          const t=(playing&&!reduced.matches?(time*.21+b.x*.013+20)%1:.47);
          const f=clamp(t,0,reveal),u=1-f;
          g.beginPath();g.arc(u*u*p.x+2*u*f*mx+f*f*q.x,u*u*p.y+2*u*f*my+f*f*q.y,1.55,0,Math.PI*2);
          g.fillStyle=rgb(colors[b.family],.9);g.fill();
        }
      }
      spots.sort((a,b)=>a.z-b.z);
      for(const p of spots)renderSphere(p,g,colors[p.node.family],p.node===selected,reduced.matches?0:time*2.2,light,p.alpha*clamp(.95+p.z*.0009,.65,1));
      labelNodes(spots,near);
    }
    function labelNodes(points,near){
      const placed=[];
      const priority=p=>(p.node===selected?20:0)+(p.node===hover?10:0)+(near.has(p.node)?2:0)+p.node.weight;
      const clean=points.slice().sort((a,b)=>priority(b)-priority(a));
      for(const p of clean){
        const n=p.node,focus=n===selected,hot=n===hover;
        if(p.alpha<.58||(!focus&&!hot&&placed.length>=(compact()?6:15)))continue;
        const size=focus?(compact()?14:17):compact()?11:12;
        g.font=(focus?650:490)+' '+size+'px -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif';
        const tw=g.measureText(n.name).width;
        let x=p.x+(focus?0:p.r+7),y=p.y+(focus?p.r*1.45+12:4);
        if(focus)x-=tw/2;
        if(x<7||x+tw>width-7||y<80||y>height-97)continue;
        const box={x:x-4,y:y-size-3,w:tw+8,h:size+8};
        if(!hot&&placed.some(b=>box.x<b.x+b.w&&box.x+box.w>b.x&&box.y<b.y+b.h&&box.y+box.h>b.y))continue;
        placed.push(box);
        g.lineWidth=3.3;g.lineJoin='round';g.strokeStyle=palette.backdrop;g.strokeText(n.name,x,y);
        g.fillStyle=focus||hot||near.has(n)?palette.ink:palette.secondary;g.fillText(n.name,x,y);
      }
    }
    function results(){
      const query=searchInput.value.trim().toLocaleLowerCase('ru');
      const matches=available.filter(n=>n.born<=time&&n.name.toLocaleLowerCase('ru').includes(query));
      $('.kg-results').innerHTML=matches.map(n=>'<button type="button" data-key="'+n.key+'"><i '+dot(n.family)+'></i><span>'+n.name+'</span><small>'+tag(n)+'</small></button>').join('')||'<p>Совпадений нет на выбранную дату.</p>';
    }
    function reset(){yaw=-.12;pitch=-.08;zoom=1;cameraTo(selected);}
    function selectAction(action){
      switch(action){
      case 'theme':toggleTheme();break;
      case 'play':play();break;
      case 'speed':speed=speed===1?2:speed===2?.5:1;$('.kg-speed').textContent=speed+'×';break;
      case 'panel':inspectorOpen=!inspectorOpen;updateInspector();schedule();break;
      case 'plus':zoom=clamp(zoom*1.16,.50,2.8);schedule();break;
      case 'minus':zoom=clamp(zoom/1.16,.50,2.8);schedule();break;
      case 'fit':reset();break;
      case 'task':api.openTask(root.querySelector('[data-kg="task"]')?.dataset.taskId);break;
      case 'source':api.go('doc',selected.key==='invitation' ? api.taskInfo('KB-014')?.sourceKey || selected.source : selected.source);break;
      case 'search':if(playing){playing=false;updatePlay();}searchInput.value='';results();finder.showModal();searchInput.focus();break;
      case 'close-search':finder.close();break;
      }
    }
    on(root,'click',e=>{
      if(e.target===finder){finder.close();return;}
      const item=e.target.closest('button');if(!item)return;
      if(item.dataset.key)choose(nodeFor(item.dataset.key));
      else if(item.dataset.kg)selectAction(item.dataset.kg);
    });
    on(searchInput,'input',results);
    on(range,'input',()=>{playing=false;updatePlay();setTime(Number(range.value));});
    function pointAt(e){
      const bounds=canvas.getBoundingClientRect(),mx=e.clientX-bounds.left,my=e.clientY-bounds.top;
      return spots.slice().reverse().find(p=>p.alpha>.25&&Math.hypot(mx-p.x,my-p.y)<=Math.max(14,p.r+3))?.node;
    }
    on(canvas,'pointerdown',e=>{
      if(drag||e.button!==0)return;
      drag={id:e.pointerId,x:e.clientX,y:e.clientY,ox:e.clientX,oy:e.clientY,moved:false};canvas.setPointerCapture(e.pointerId);
      canvas.focus({preventScroll:true});
    });
    on(canvas,'pointermove',e=>{
      if(!drag){const next=pointAt(e);if(next!==hover){hover=next;canvas.style.cursor=next?'pointer':'grab';schedule();}return;}
      if(e.pointerId!==drag.id)return;
      if(Math.hypot(e.clientX-drag.ox,e.clientY-drag.oy)>4)drag.moved=true;
      if(drag.moved){yaw+=(e.clientX-drag.x)*.007;pitch=clamp(pitch+(e.clientY-drag.y)*.007,-1.48,1.48);canvas.style.cursor='grabbing';schedule();}
      drag.x=e.clientX;drag.y=e.clientY;
    });
    on(canvas,'pointerup',e=>{if(!drag||e.pointerId!==drag.id)return;const click=!drag.moved;drag=null;canvas.style.cursor='grab';if(click){const node=pointAt(e);if(node)choose(node);}});
    on(canvas,'lostpointercapture',()=>{drag=null;canvas.style.cursor='grab';});
    on(canvas,'pointerleave',()=>{if(hover){hover=null;schedule();}});
    on(canvas,'wheel',e=>{e.preventDefault();zoom=clamp(zoom*Math.exp(-e.deltaY*.001),.50,2.8);schedule();},{passive:false});
    on(canvas,'keydown',e=>{
      const keys={ArrowLeft:()=>yaw-=.12,ArrowRight:()=>yaw+=.12,ArrowUp:()=>pitch=clamp(pitch-.12,-1.48,1.48),ArrowDown:()=>pitch=clamp(pitch+.12,-1.48,1.48),'+':()=>selectAction('plus'),'=':()=>selectAction('plus'),'-':()=>selectAction('minus'),' ':play,Home:reset,Enter:()=>selectAction('search')};
      if(keys[e.key]){e.preventDefault();keys[e.key]();schedule();}
    });
    function size(){
      width=root.clientWidth;height=root.clientHeight;
      const dpr=Math.min(window.devicePixelRatio||1,2);
      canvas.width=Math.max(1,Math.round(width*dpr));canvas.height=Math.max(1,Math.round(height*dpr));g?.setTransform(dpr,0,0,dpr,0,0);
      schedule();
    }
    const resize=new ResizeObserver(size);resize.observe(root);
    on(reduced,'change',()=>{
      if(reduced.matches&&cameraTarget){pan=cameraTarget;cameraTarget=null;}
      schedule();
    });
    updateThemeButton();updateInspector();updateHistory();updatePlay();size();
    on(document,'visibilitychange',()=>{if(document.hidden&&playing){playing=false;updatePlay();}});
    return ()=>{delete document.body.dataset.graphTheme;disposed=true;abort.abort();resize.disconnect();cancelAnimationFrame(frame);if(finder.open)finder.close();};
  }
  window.KBGraph={mount};
})();
