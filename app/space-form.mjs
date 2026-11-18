const dialog = document.querySelector('#space-dialog, #section-dialog');
const creatingSection = dialog.id === 'section-dialog';
const form = dialog.querySelector('form');
const nameField = form.elements.namedItem('name');
const submit = form.querySelector('button[type="submit"]');
const status = dialog.querySelector('[data-space-status], [data-section-status]');
const cancelLinks = dialog.querySelectorAll('[data-space-cancel], [data-section-cancel]');
const inlineError = form.querySelector('#space-client-error, #section-client-error');
const serverDescription = nameField.getAttribute('aria-describedby');
const serverInvalid = nameField.getAttribute('aria-invalid');
const maxNameLength = 100;
const maxFormBytes = 4_096;
const encodedFormLength = formData => new URLSearchParams(formData).toString().length;

if (typeof dialog.showModal === 'function') {
  dialog.removeAttribute('open');
  dialog.showModal();
}
nameField.focus();

function setSaving(saving) {
  submit.disabled = saving;
  submit.textContent = saving ? 'Создаём…' : 'Создать';
  nameField.readOnly = saving;
  form.setAttribute('aria-busy', String(saving));
  status.textContent = saving ? (creatingSection ? 'Создаём раздел.' : 'Создаём пространство.') : '';
  for (const link of cancelLinks) {
    if (saving) link.setAttribute('aria-disabled', 'true');
    else link.removeAttribute('aria-disabled');
  }
}

form.addEventListener('submit', event => {
  if (submit.disabled) {
    event.preventDefault();
    return;
  }
  const enteredName = nameField.value;
  const name = enteredName.trim();
  const tooLong = Array.from(name).length > maxNameLength;
  const oversizedBlank = !name && enteredName.length > 0 && encodedFormLength(new FormData(form)) > maxFormBytes;
  if (tooLong || oversizedBlank) {
    event.preventDefault();
    inlineError.textContent = tooLong ? 'Введите не больше 100 символов.' : (creatingSection ? 'Введите название раздела.' : 'Введите название пространства.');
    inlineError.hidden = false;
    nameField.setAttribute('aria-invalid', 'true');
    nameField.setAttribute('aria-describedby', creatingSection ? 'section-help section-client-error' : 'space-help space-client-error');
    nameField.focus();
    return;
  }
  setSaving(true);
});

form.addEventListener('formdata', ({formData}) => {
  const enteredName = formData.get('name');
  if (typeof enteredName !== 'string') return;
  const name = enteredName.trim();
  if (name !== enteredName && name.length > 0 && Array.from(name).length <= maxNameLength &&
      encodedFormLength(formData) > maxFormBytes) {
    formData.set('name', name);
  }
});

nameField.addEventListener('input', () => {
  if (inlineError.hidden) return;
  inlineError.hidden = true;
  nameField.setAttribute('aria-describedby', serverDescription);
  if (serverInvalid) nameField.setAttribute('aria-invalid', serverInvalid);
  else nameField.removeAttribute('aria-invalid');
});

dialog.addEventListener('cancel', event => {
  event.preventDefault();
  if (!submit.disabled) window.location.assign(cancelLinks[0].href);
});

for (const link of cancelLinks) link.addEventListener('click', event => {
  if (submit.disabled) event.preventDefault();
});

window.addEventListener('pageshow', () => setSaving(false));
