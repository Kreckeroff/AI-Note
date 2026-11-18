import {accountIds, canSeeTask, documentById, ownsSpace, profileFor, spaceInfo, taskStatuses} from './model.mjs';
import {avatar, button, entity, escapeHtml as html, icon, selectOptions} from './ui.mjs';

export function visibleTasks(data) {
  return data.tasks.filter(task => canSeeTask(data,task));
}

function taskCard(data,task) {
  const source = documentById(data,task.sourceKey), person = profileFor(data,task.person);
  return `<article class="task-card" draggable="${ownsSpace(data)}" data-drag-task="${html(task.id)}">
    <button type="button" class="task-open" data-action="task" data-value="${html(task.id)}">
      <span class="mono muted">${html(task.id)}</span><strong>${html(task.title)}</strong>
    </button>
    <div class="task-attributes">${source ? `<button type="button" class="task-source" data-action="open-doc" data-value="${html(source.id)}">${icon('doc',13)}<span>${html(source.title)}</span></button>` : '<span>Без источника</span>'}
      ${person ? `<span title="${html(person.name)}">${avatar(person,true)}</span>` : ''}</div>
  </article>`;
}

export function renderBoard(data,view) {
  const tasks = visibleTasks(data).filter(task => !view.boardPerson || task.person === view.boardPerson);
  const lane = (key,label) => {
    const items = tasks.filter(task => task.status === key);
    return `<section class="lane" data-status="${key}" aria-label="${label}">
      <header class="lane-head"><span class="status-dot"></span><h2>${label}</h2><span class="mono">${items.length}</span>
        ${ownsSpace(data) ? button('create-task','','plus',{className:'ghost',title:'Добавить задачу в «'+label+'»',value:key}) : ''}</header>
      ${items.length ? items.map(task => taskCard(data,task)).join('') : '<p class="lane-empty">Задач пока нет.</p>'}</section>`;
  };
  return `<section class="board-view appearance" aria-label="Задачи команды">
    <header class="view-header"><div><div class="view-kicker">${icon('board',15)} ${html(spaceInfo(data).name)}${data.space === 'north' ? ' / Онбординг' : ''}</div><h1>Задачи команды</h1>
      <p class="muted">Проверка доступа и передача материалов</p></div>
      <div class="view-actions">${ownsSpace(data) ? button('create-task','Новая задача','plus',{className:'primary',value:'open'}) : ''}</div>
    </header>
    <div class="board-tools">
      <div class="view-actions">${button('board-mode','Kanban','board',{pressed:view.boardMode === 'board',value:'board'})}${button('board-mode','Список','list',{pressed:view.boardMode === 'list',value:'list'})}</div>
      <label><span class="sr-only">Исполнитель</span><select class="field compact" data-control="board-person">${selectOptions({'':'Все исполнители',...Object.fromEntries(accountIds(data).map(key=>[key,profileFor(data,key).name]))},view.boardPerson)}</select></label>
    </div>
    ${view.boardMode === 'board' ? `<div class="board-lanes">${Object.entries(taskStatuses).map(([key,label]) => lane(key,label)).join('')}</div>` :
      `<div class="board-list">${tasks.map(task => entity('task',task.id,'board',task.title,task.id+' · '+taskStatuses[task.status],'work')).join('') || '<p class="muted">Нет подходящих задач.</p>'}</div>`}
  </section>`;
}
