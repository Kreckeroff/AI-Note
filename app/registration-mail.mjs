import {spawn} from 'node:child_process';
import {accessSync, constants, statSync} from 'node:fs';
import {isAbsolute} from 'node:path';
import {normalizeEmail} from './identity.mjs';

export function createRegistrationDelivery(environment, deliver) {
  const value = environment.SEVER_SELF_REGISTRATION;
  if (value === undefined || value === 'false') return undefined;
  if (value !== 'true') throw new Error('SEVER_SELF_REGISTRATION принимает только true или false.');
  if (deliver) return deliver;

  const executable = environment.SEVER_REGISTRATION_SENDMAIL || '';
  try {
    if (!isAbsolute(executable) || !statSync(executable).isFile()) throw new Error();
    accessSync(executable, constants.X_OK);
  } catch {
    throw new Error('В SEVER_REGISTRATION_SENDMAIL укажите абсолютный путь к исполняемому sendmail.');
  }
  const from = normalizeEmail(environment.SEVER_REGISTRATION_FROM || '');
  if (!from) throw new Error('В SEVER_REGISTRATION_FROM укажите действительный адрес отправителя.');

  return ({email, link, expiresInMinutes}) => new Promise((resolve, reject) => {
    const subject = Buffer.from('Подтвердите почту для регистрации в «Севере»').toString('base64');
    const message = [
      `From: <${from}>`,
      `To: <${email}>`,
      `Subject: =?UTF-8?B?${subject}?=`,
      'MIME-Version: 1.0',
      'Content-Type: text/plain; charset=UTF-8',
      'Content-Transfer-Encoding: 8bit',
      '',
      'Чтобы завершить регистрацию в «Севере», откройте ссылку на компьютере, где запущено приложение.',
      link,
      `Срок действия ссылки — ${expiresInMinutes} мин. Если вы не запрашивали регистрацию, письмо можно проигнорировать.`,
      '',
    ].join('\n');
    const mail = spawn(executable, ['-i', '-t'], {stdio: ['pipe', 'ignore', 'ignore'], timeout: 5000, killSignal: 'SIGKILL'});
    mail.once('error', reject);
    mail.stdin.once('error', reject);
    mail.once('close', code => {
      if (code === 0) resolve();
      else reject(new Error('Sendmail не принял сообщение.'));
    });
    mail.stdin.end(message);
  });
}
