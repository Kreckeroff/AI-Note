(() => {
  const editor = 'form[data-native-page-editor]';
  const pending = 'data-page-editor-pending';

  function updateActions(form) {
    const saving = form.hasAttribute(pending);
    const field = form.elements.namedItem('text');
    const rebase = form.elements.namedItem('baseRevision');
    const save = form.querySelector('button[type="submit"]');
    const cancel = form.querySelector('.page-editor-actions a');
    save.disabled = saving || Boolean(rebase && !rebase.checked);
    field.readOnly = saving;
    if (saving) cancel?.setAttribute('aria-disabled', 'true');
    else cancel?.removeAttribute('aria-disabled');
    form.querySelector('.page-editor-status').textContent = saving ? 'Сохраняем…' : '';
  }

  document.addEventListener('submit', event => {
    const form = event.target;
    if (event.defaultPrevented || !form.matches(editor)) return;
    const rebase = form.elements.namedItem('baseRevision');
    if (form.hasAttribute(pending) || (rebase && !rebase.checked)) {
      event.preventDefault();
      return;
    }
    const field = form.elements.namedItem('text');
    const format = form.elements.namedItem('contentFormat');
    const mode = form.elements.namedItem('editorMode');
    // A fresh native field edits Markdown. A retry must keep the format and mode already submitted.
    const editsSource = form.hasAttribute('data-native-source') && field.value !== field.defaultValue;
    const literal = !editsSource && format.value === 'literal';
    const tooLong = Array.from(field.value).length > 60_000;
    const literalText = literal ? field.value.replace(/\\([\x21-\x2f\x3a-\x40\x5b-\x60\x7b-\x7e])/g, '$1') : '';
    if (tooLong || (literal && Array.from(literalText).length > 30_000)) {
      event.preventDefault();
      field.setCustomValidity(tooLong ? 'Введите не больше 60 000 символов.' : 'Введите не больше 30 000 символов.');
      field.reportValidity();
      field.focus();
      return;
    }
    if (editsSource) {
      format.value = 'markdown';
      mode.value = 'source';
    }
    form.setAttribute(pending, '');
    updateActions(form);
  });

  document.addEventListener('click', event => {
    const control = event.target.closest('a, input[name="baseRevision"]');
    if (control?.closest(`${editor}[${pending}]`)) event.preventDefault();
  });

  document.addEventListener('change', ({target}) => {
    if (target.matches(`${editor} input[name="baseRevision"]`)) updateActions(target.form);
  });

  document.addEventListener('input', ({target}) => {
    if (target.matches(`${editor} textarea[name="text"]`)) target.setCustomValidity('');
  });

  document.addEventListener('readystatechange', () => {
    if (document.readyState === 'interactive') document.querySelectorAll(editor).forEach(updateActions);
  });

  window.addEventListener('pageshow', event => {
    if (!event.persisted) return;
    for (const form of document.querySelectorAll(editor)) {
      form.removeAttribute(pending);
      updateActions(form);
    }
  });
})();
