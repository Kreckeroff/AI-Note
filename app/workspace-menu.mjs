const choices = [...document.querySelectorAll('[data-workspace-choice]')];
const search = document.querySelector('[data-picker-search]');
const input = search?.querySelector('input');
const groups = [...document.querySelectorAll('[data-picker-group]')];
const empty = document.querySelector('[data-picker-empty]');

if (window.location.hash === '#create-section' && document.activeElement === document.body) {
  document.getElementById('create-section')?.focus();
}

function fold(value) {
  return value.normalize('NFC').toLowerCase().normalize('NFC');
}

function filterSpaces() {
  const query = fold(input.value.trim());
  let found = false;
  for (const group of groups) {
    let visible = 0;
    for (const link of group.querySelectorAll('[data-picker-space]')) {
      const matches = query ? fold(link.querySelector('[data-picker-label]').textContent).includes(query)
        : visible < 4 || link.getAttribute('aria-current') === 'page';
      link.parentElement.hidden = !matches;
      if (matches) visible++;
    }
    group.hidden = visible === 0;
    found ||= visible > 0;
  }
  empty.hidden = found;
}

if (input) {
  search.hidden = false;
  filterSpaces();
  input.addEventListener('input', filterSpaces);
}

for (const choice of choices) {
  choice.addEventListener('toggle', () => {
    if (!choice.open) return;
    for (const other of choices) if (other !== choice) other.open = false;
    if (input && choice.contains(input)) {
      input.value = '';
      filterSpaces();
      input.focus();
    }
  });
  choice.addEventListener('keydown', event => {
    if (event.key !== 'Escape' || !choice.open) return;
    event.preventDefault();
    choice.open = false;
    choice.querySelector('summary').focus();
  });
}

document.addEventListener('click', event => {
  for (const choice of choices) if (choice.open && !choice.contains(event.target)) choice.open = false;
});

const catalog = document.querySelector('[data-catalog]');
if (catalog) {
  const count = catalog.querySelector('[data-catalog-count]');
  const originalCount = count.textContent;
  const loading = () => { count.textContent = 'Загружаем пространства…'; };
  catalog.querySelector('[data-catalog-search]').addEventListener('submit', loading);
  catalog.addEventListener('click', event => {
    if (!event.defaultPrevented && event.button === 0 && !event.ctrlKey && !event.metaKey && !event.shiftKey && !event.altKey &&
      event.target.closest('[data-catalog-nav]')) loading();
  });
  window.addEventListener('pageshow', () => { count.textContent = originalCount; });
}
