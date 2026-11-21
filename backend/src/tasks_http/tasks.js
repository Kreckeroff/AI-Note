(() => {
  const board = document.querySelector('.tasks-board');
  const panel = document.querySelector('[data-task-panel]');
  if (!board) return;
  if (panel) {
    panel.setAttribute('role', 'dialog');
    panel.setAttribute('aria-modal', 'true');
    for (let branch = panel; branch.parentElement && branch.parentElement !== document.documentElement; branch = branch.parentElement) {
      for (const sibling of branch.parentElement.children) {
        if (sibling !== branch) sibling.inert = true;
      }
    }
    const focusable = () => Array.from(panel.querySelectorAll('input:not([type="hidden"]), select, button, a[href]'));
    const close = panel.querySelector('[aria-label="Закрыть"]');
    (panel.querySelector('input[name="title"]') ?? panel).focus();
    panel.addEventListener('keydown', event => {
      if (event.key === 'Escape') {
        event.preventDefault();
        close.click();
      }
      if (event.key === 'Tab') {
        const controls = focusable();
        const edge = event.shiftKey ? controls[0] : controls.at(-1);
        if (document.activeElement === edge) {
          event.preventDefault();
          (event.shiftKey ? controls.at(-1) : controls[0]).focus();
        }
      }
    });
    return;
  }
  const filters = board.querySelector('.tasks-filters');
  filters?.addEventListener('change', event => {
    if (event.target instanceof HTMLSelectElement) filters.requestSubmit();
  });
  const form = board.querySelector('form[data-task-drop-form]');
  let drag = null;
  let highlighted = null;
  function highlight(lane) {
    if (highlighted === lane) return;
    highlighted?.removeAttribute('data-drop-active');
    highlighted = lane;
    highlighted?.setAttribute('data-drop-active', '');
  }
  const laneFor = target => target instanceof Element ? target.closest('[data-task-lane]') : null;
  board.addEventListener('dragstart', event => {
    const card = event.target instanceof Element ? event.target.closest('[data-task-drag]') : null;
    if (!card) {
      event.preventDefault();
      return;
    }
    drag = {
      action: card.dataset.taskDrag,
      version: card.dataset.taskVersion,
      status: card.dataset.taskStatus,
    };
    event.dataTransfer.effectAllowed = 'move';
    event.dataTransfer.setData('text/plain', 'task');
  });
  board.addEventListener('dragend', () => {
    drag = null;
    highlight(null);
  });
  board.addEventListener('dragover', event => {
    const lane = laneFor(event.target);
    if (!drag || !lane || lane.dataset.taskLane === drag.status) {
      highlight(null);
      return;
    }
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
    highlight(lane);
  });
  board.addEventListener('dragleave', event => {
    if (laneFor(event.target) !== laneFor(event.relatedTarget)) highlight(null);
  });
  board.addEventListener('drop', event => {
    const lane = laneFor(event.target);
    highlight(null);
    if (!drag || !lane || lane.dataset.taskLane === drag.status) return;
    event.preventDefault();
    form.action = drag.action;
    form.elements.namedItem('version').value = drag.version;
    form.elements.namedItem('status').value = lane.dataset.taskLane;
    form.requestSubmit();
    drag = null;
  });
})();
