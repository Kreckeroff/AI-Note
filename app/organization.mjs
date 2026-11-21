const logoForm = document.querySelector('[data-org-logo]');
const preview = document.querySelector('[data-org-logo-preview]');

if (logoForm instanceof HTMLFormElement && preview instanceof HTMLImageElement) {
  const fileInput = logoForm.elements.namedItem('logo');
  const status = logoForm.querySelector('[data-org-logo-status]');
  const submit = logoForm.querySelector('button');
  const originalSource = preview.src;
  let objectUrl;
  let validFile;

  function resetPreview() {
    if (objectUrl) URL.revokeObjectURL(objectUrl);
    objectUrl = undefined;
    preview.src = originalSource;
  }

  fileInput?.addEventListener('change', () => {
    resetPreview();
    validFile = null;
    const file = fileInput.files?.[0];
    if (!file) {
      status.textContent = '';
      return;
    }
    if (file.size > 5 * 1024 * 1024 || !['image/png', 'image/jpeg', 'image/webp'].includes(file.type)) {
      status.textContent = 'Выберите PNG, JPEG или WebP до 5 МБ.';
      return;
    }
    validFile = file;
    objectUrl = URL.createObjectURL(file);
    preview.src = objectUrl;
    status.textContent = 'Предпросмотр логотипа. Сохраните, чтобы применить.';
  });

  logoForm.addEventListener('submit', async event => {
    event.preventDefault();
    if (!validFile || submit.disabled) return;
    submit.disabled = true;
    status.textContent = 'Сохраняем…';
    try {
      const response = await fetch(logoForm.action, {
        method: 'POST',
        credentials: 'same-origin',
        headers: {'Content-Type': validFile.type},
        body: validFile,
      });
      if (response.redirected && response.ok) {
        location.assign(response.url);
      } else {
        status.textContent = 'Не удалось сохранить логотип. Убедитесь, что файл открывается и у вас есть права.';
      }
    } catch {
      status.textContent = 'Не удалось отправить файл. Повторите попытку.';
    } finally {
      submit.disabled = false;
    }
  });
}
