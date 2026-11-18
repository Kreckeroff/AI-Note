import {openAccountStore} from './account-store.mjs';
import {passwordMaxLength, provisionAccount} from './identity.mjs';

async function readPassword(input) {
  const chunks = [];
  let length = 0;
  for await (const chunk of input) {
    length += chunk.length;
    if (length > passwordMaxLength * 4 + 2) throw new Error('Слишком длинный пароль.');
    chunks.push(chunk);
  }
  return new TextDecoder('utf-8', {fatal: true, ignoreBOM: true}).decode(Buffer.concat(chunks)).replace(/\r?\n$/u, '');
}

const [email, name, ...other] = process.argv.slice(2);
if (!email || !name || other.length || process.stdin.isTTY) {
  console.error('Использование: node app/create-account.mjs <почта> <имя>. Передайте пароль через stdin.');
  process.exitCode = 1;
} else {
  let store;
  try {
    const password = await readPassword(process.stdin);
    store = openAccountStore();
    await provisionAccount(store, {email, name, password});
    console.log('Учётная запись создана.');
  } catch (error) {
    console.error(error.code || error instanceof TypeError ? 'Не удалось создать учётную запись.' : error.message);
    process.exitCode = 1;
  } finally {
    store?.close();
  }
}
