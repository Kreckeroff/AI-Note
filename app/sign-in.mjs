const form = document.querySelector('form');
const submit = form.querySelector('button[type=submit]');
const status = document.querySelector('[data-sign-in-status]');

form.addEventListener('submit', event => {
  if (submit.disabled) {
    event.preventDefault();
    return;
  }
  submit.disabled = true;
  submit.textContent = 'Входим…';
  form.setAttribute('aria-busy', 'true');
  status.textContent = 'Проверяем данные входа.';
});

window.addEventListener('pageshow', () => {
  submit.disabled = false;
  submit.textContent = 'Войти';
  form.removeAttribute('aria-busy');
  status.textContent = '';
});
