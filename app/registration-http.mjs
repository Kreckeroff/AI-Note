import {formValue, hasFormEncoding, plain, readForm, redirect, send} from './http.mjs';
import {emailMaxLength} from './identity.mjs';
import {registrationConfirmationPath} from './registration-paths.mjs';
import {registrationCompletePage, registrationRequestedPage, registrationStartPage} from './views.mjs';
import {workspacePath} from './workspace-paths.mjs';

const privateHeaders = {'Referrer-Policy': 'no-referrer'};
// The browser sends the origin on form POST while keeping the confirmation query out of Referer.
const confirmationFormHeaders = {'Referrer-Policy': 'strict-origin'};

export function createRegistrationHttp(registration, {sessionCookie, record}) {
  const invalidLink = response => plain(response, 404, 'Ссылка недействительна. Начните регистрацию заново.', privateHeaders);

  return async (request, response, {path, origin}) => {
    const confirming = path === registrationConfirmationPath;
    if (request.method === 'GET' || request.method === 'HEAD') {
      if (!confirming) return send(response, 200, registrationStartPage());
      const token = formValue(new URL(request.url, origin).searchParams, 'token');
      if (!registration.isPending(token)) return invalidLink(response);
      return send(response, 200, registrationCompletePage({token}), confirmationFormHeaders);
    }
    if (request.method !== 'POST') return plain(response, 405, 'Метод недоступен.', {Allow: 'GET, HEAD, POST', ...privateHeaders});
    if (request.headers.origin !== origin) return plain(response, 403, 'Откройте форму регистрации в «Север».', privateHeaders);
    if (!hasFormEncoding(request)) return plain(response, 415, 'Откройте форму регистрации в «Север».', privateHeaders);
    const form = await readForm(request);
    if (!form) return plain(response, 413, 'Форма регистрации слишком большая.', {Connection: 'close', ...privateHeaders});

    if (!confirming) {
      const email = formValue(form, 'email');
      const outcome = registration.request(email, token => `${origin}${registrationConfirmationPath}?token=${token}`);
      if (outcome === 'invalid-email') {
        return send(response, 422, registrationStartPage({email: email.slice(0, emailMaxLength), error: 'Укажите действительный адрес почты.'}));
      }
      return send(response, 202, registrationRequestedPage());
    }

    const token = formValue(form, 'token');
    const name = formValue(form, 'name');
    const result = await registration.confirm({token, name, password: formValue(form, 'password')});
    if (result.outcome === 'invalid-link') return invalidLink(response);
    if (result.outcome === 'invalid-fields') {
      return send(response, 422, registrationCompletePage({token, name, error: result.message}), confirmationFormHeaders);
    }
    if (result.outcome === 'limited') {
      const error = 'Подождите несколько секунд и попробуйте снова.';
      return send(response, 429, registrationCompletePage({token, name, error}), {'Retry-After': '2', ...confirmationFormHeaders});
    }
    record('registration.completed', {accountId: result.accountId, tenantId: result.tenantId});
    return redirect(response, workspacePath, {'Set-Cookie': sessionCookie(result.token), ...privateHeaders});
  };
}
