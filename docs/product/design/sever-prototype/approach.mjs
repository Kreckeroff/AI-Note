import {button, entity, escapeHtml as html, icon, selectOptions} from './ui.mjs';
import {accessFor, documentById, profileFor} from './model.mjs';

export const retrievalLabels = {direct:'Закреплённый источник', search:'Поиск по документам', rag:'RAG по источникам'};
const baseNodes = [
  {id:'trigger',type:'Вход',title:'Задача KB-014',tone:'work',icon:'board',x:18,y:57,description:'Задаёт задачу, область работы и исполнителя.'},
  {id:'instruction',type:'Инструкция',title:'Проверить приглашение',tone:'agent',icon:'spark',x:207,y:57,description:'Текст системной инструкции для аналитика.'},
  {id:'source',type:'Контекст',title:'Найти основание',tone:'document',icon:'book',x:396,y:57,description:'Определяет доступный источник и способ подбора фрагментов.'},
  {id:'action',type:'Инструмент',title:'Подготовить предложение',tone:'knowledge',icon:'code',x:585,y:57,description:'Агент может создать черновик хэндоффа в текущем пространстве.'},
  {id:'review',type:'Подтверждение',title:'Решение владельца',tone:'work',icon:'user',x:396,y:269,description:'Владелец проверяет предложение и подтверждает область доступа.'},
  {id:'output',type:'Результат',title:'Хэндофф аналитика',tone:'document',icon:'doc',x:585,y:269,description:'Передаёт проверенный материал и ссылку на исходный документ.'},
];
const conditional = {id:'guard',type:'Условие',title:'Есть ссылка на источник?',tone:'knowledge',icon:'shield',x:207,y:269,description:'Продолжение возможно, если предложение содержит доступный источник.'};
const getNodes = spec => spec.guard ? [...baseNodes,conditional] : baseNodes;
export function getRunSequence(spec) {
  return ['trigger','instruction','source','action',...(spec.guard ? ['guard'] : []),'review','output'];
}
export function approachSource(data) {
  const doc = documentById(data,data.approach.source);
  return accessFor(data,doc) !== 'none' ? doc : null;
}
export function agentSnapshot(data) {
  const {instruction,retrieval,version,guard} = data.approach;
  const source = approachSource(data);
  return [
    '# Проверка приглашения', 'Редакция '+version, '', '## Задача', 'KB-014 · Вход по приглашению', '',
    '## Инструкция', instruction, '', '## Контекст', retrievalLabels[retrieval],
    source ? source.title+' · редакция '+source.revision : 'Доступный источник не выбран.',
    '', '## Разрешённые действия', '- Прочитать выбранный источник.', '- Подготовить черновик хэндоффа.',
    ...(guard ? ['- Включить ссылку на доступный источник.'] : []),
    '- Передать предложение на подтверждение владельцу.',
  ].join('\n');
}
export function logLabel(nodeId,data) {
  switch (nodeId) {
    case 'trigger': return 'Задача KB-014 получена';
    case 'instruction': return 'Инструкция применена';
    case 'source': return retrievalLabels[data.approach.retrieval] + ': источник доступен';
    case 'action': return 'Черновик предложения подготовлен';
    case 'guard': return 'Ссылка на источник найдена';
    case 'review': return 'Подтверждение владельца смоделировано';
    default: return 'Хэндофф собран';
  }
}
function edgePath(a,b) {
  const startX = a.x+156, startY = a.y+42, endX = b.x, endY = b.y+42;
  if (a.y === b.y) return `M${startX} ${startY}H${endX}`;
  const lowerY = (startY+endY)/2, outerX = Math.min(startX+8,758), beforeX = endX-10;
  return `M${startX} ${startY}H${outerX} Q${outerX+2} ${startY} ${outerX+2} ${startY+7} V${lowerY-11} Q${outerX+2} ${lowerY} ${outerX-8} ${lowerY} H${beforeX+10} Q${beforeX} ${lowerY} ${beforeX} ${lowerY+10} V${endY-4} Q${beforeX} ${endY} ${beforeX+5} ${endY} H${endX}`;
}

function flowGraph(spec,run,source,owner) {
  const sequence = getRunSequence(spec);
  const nodes = getNodes(spec);
  const edges = sequence.slice(1).map((id,index) => {
    const a = nodes.find(node => node.id === sequence[index]);
    const b = nodes.find(node => node.id === id);
    return `<path class="${run.steps.includes(a.id) ? 'active' : ''}" d="${edgePath(a,b)}"/>`;
  }).join('');
  return `<div class="flow-canvas">
    <svg class="flow-edges" viewBox="0 0 760 452" aria-hidden="true">${edges}</svg>
    ${nodes.map(node => {
      const detail = node.id === 'source' ? (source?.title || 'Выберите источник') :
        node.id === 'trigger' ? 'Задача и пространство' : node.id === 'instruction' ? 'Правила для аналитика' :
        node.id === 'guard' ? 'Проверка ссылки' : node.id === 'action' ? 'Создание черновика' : node.id === 'review' ? owner : 'Документ + источник';
      return `<button class="flow-node" type="button" data-action="flow-node" data-value="${node.id}" data-tone="${node.tone}"
        style="--x:${node.x}px;--y:${node.y}px" aria-pressed="${spec.node === node.id}" data-running="${run.current === node.id}">
        <span class="flow-node-head"><span class="entity-icon">${icon(node.icon,12)}</span>${node.type}</span>
        <strong>${node.title}</strong><small>${html(detail)}</small></button>`;
    }).join('')}
  </div>`;
}

function nodeProperties(data,node,source,running) {
  const spec = data.approach;
  if (node.id === 'instruction') {
    return `<label><span>Текст инструкции</span><textarea class="field" data-approach="instruction" rows="6" maxlength="1000"${running ? ' disabled' : ''}>${html(spec.instruction)}</textarea></label>`;
  }
  if (node.id === 'source') {
    const pages = Object.fromEntries(data.documents.filter(doc => doc.kind === 'page' && doc.space === data.space && accessFor(data,doc) !== 'none').map(doc => [doc.id,doc.title]));
    return `<label><span>Способ подбора</span><select class="field" data-approach="retrieval"${running ? ' disabled' : ''}>${selectOptions(retrievalLabels,spec.retrieval)}</select></label>
      <label><span>Исходная страница</span><select class="field" data-approach="source"${running ? ' disabled' : ''}>${selectOptions({'':'Выберите страницу',...pages},spec.source)}</select></label>
      ${source ? entity('open-doc',source.id,'doc',source.title,'Редакция '+source.revision) : ''}`;
  }
  if (node.id === 'trigger') return entity('task','KB-014','board','Вход по приглашению','KB-014','work');
  if (node.id === 'output' && accessFor(data,documentById(data,'handoff')) !== 'none') return entity('open-doc','handoff','doc','Хэндофф аналитика','Открыть образец результата');
  if (node.id === 'guard') return button('flow-guard','Удалить условие','close',{className:'danger',disabled:running});
  return '';
}

export function flowInspector(data,run) {
  const source = approachSource(data);
  const node = getNodes(data.approach).find(item => item.id === data.approach.node) || baseNodes[1];
  return `<span class="tag" data-tone="${node.tone}">${icon(node.icon,12)} ${node.type}</span>
    <h2>${node.title}</h2><p>${node.description}</p>${nodeProperties(data,node,source,run.running)}
    ${run.error ? `<p class="form-status" role="alert">${html(run.error)}</p>` : ''}
    ${run.steps.length ? `<hr class="rule"><h3 class="small">Журнал прогона</h3>
    <ol class="run-log" aria-label="Завершённые шаги">${run.steps.map(id => `<li>${icon('check',12)}<span>${html(logLabel(id,data))}</span></li>`).join('')}</ol>` : ''}
    ${run.completed ? `<div class="dialog-buttons">${button('flow-handoff','Передать на доску','arrow',{className:'primary'})}</div>` : ''}`;
}

export function renderApproach(data,run) {
  return `<section class="flow-view appearance">
    <header class="view-header"><div><div class="view-kicker">${icon('flow',15)} AI-подход</div><h1>Проверка приглашения</h1><p class="muted">Инструкция и рабочий контекст аналитика</p></div>
      <div class="view-actions">${button('agent-preview','Глазами агента','eye')}${button(run.running ? 'flow-stop' : 'flow-run',run.running ? 'Остановить' : 'Пробный запуск','play',{className:'primary'})}</div></header>
    <div class="flow-grid">
      <div class="flow-surface" aria-label="Холст AI-подхода">
        <div class="flow-toolbar"><span class="tag">${icon('layers',12)} Редакция ${data.approach.version}</span>
          ${button('flow-guard',data.approach.guard ? 'Убрать условие' : 'Добавить условие',data.approach.guard ? 'close' : 'plus',{className:'ghost',disabled:run.running})}</div>
        ${flowGraph(data.approach,run,approachSource(data),profileFor(data,'anna').name)}
      </div>
      <aside class="flow-inspect" aria-label="Свойства выбранного блока">${flowInspector(data,run)}</aside>
    </div>
  </section>`;
}
