import {createHash, randomBytes, randomUUID, scrypt, timingSafeEqual} from 'node:crypto';
import {promisify} from 'node:util';

export const sessionLifetime = 12 * 60 * 60 * 1000;
export const passwordMinLength = 15;
export const passwordMaxLength = 128;
export const emailMaxLength = 254;
export const nameMaxLength = 120;
// Use the same UnicodeSets-compatible pattern for HTML and server validation.
const mailbox = String.raw`[a-zA-Z0-9.!#$%&'*+\x2f=?^_\x60\x7b\x7c\x7d~\x2d]{1,64}`;
const domainLabel = String.raw`[a-zA-Z0-9](?:[a-zA-Z0-9\x2d]{0,61}[a-zA-Z0-9])?`;
export const emailPattern = `(?=.{1,${emailMaxLength}}$)${mailbox}@${domainLabel}(?:\\.${domainLabel})+`;
const emailValidator = new RegExp(`^(?:${emailPattern})$`, 'v');
const scryptAsync = promisify(scrypt);
const scryptParameters = {N: 2 ** 17, r: 8, p: 1, maxmem: 160 * 1024 * 1024};
const hashLength = 32;
const attemptWindow = 5 * 60 * 1000;
const identityAttemptLimit = 6;
const serviceAttemptLimit = 48;
const concurrentAttemptLimit = 4;

export function normalizeEmail(email) {
  const address = email.trim();
  if (emailValidator.test(address)) return address.toLowerCase();
}

function validPassword(password) {
  return password.length > 0 && Array.from(password).length <= passwordMaxLength && !/[\r\n\0]/u.test(password);
}

function digestToken(token) {
  return createHash('sha256').update(token).digest();
}

function derivePassword(password, salt) {
  return scryptAsync(password, salt, hashLength, scryptParameters);
}

export class AccountInputError extends Error {}

export async function prepareAccount({email, name, password}) {
  email = normalizeEmail(email);
  name = name.trim();
  if (!email) throw new AccountInputError('Укажите действительный адрес электронной почты.');
  if (name.length === 0 || Array.from(name).length > nameMaxLength || /[\p{Cc}]/u.test(name)) {
    throw new AccountInputError('Укажите имя длиной от 1 до 120 символов.');
  }
  if (!validPassword(password) || Array.from(password).length < passwordMinLength) {
    throw new AccountInputError('Пароль должен содержать от 15 до 128 символов в одной строке.');
  }
  const salt = randomBytes(16);
  const passwordHash = await derivePassword(password, salt);
  return {id: randomUUID(), email, name, salt, passwordHash};
}

export async function provisionAccount(store, fields) {
  const account = await prepareAccount(fields);
  if (!store.addAccount(account)) {
    throw new Error('Учётная запись с такой почтой уже существует.');
  }
  return account.id;
}

export function prepareSession(createdAt) {
  const token = randomBytes(32).toString('base64url');
  const session = {tokenDigest: digestToken(token), createdAt, expiresAt: createdAt + sessionLifetime};
  return {token, session};
}

function createAttemptBudget() {
  const attemptsByEmail = new Map();
  let serviceWindowEnd = 0;
  let serviceAttempts = 0;
  let activeAttempts = 0;

  return {
    reserve(email, now) {
      if (now >= serviceWindowEnd) {
        serviceAttempts = 0;
        serviceWindowEnd = now + attemptWindow;
        for (const [key, window] of attemptsByEmail) {
          if (now >= window.end) attemptsByEmail.delete(key);
        }
      }
      let window = attemptsByEmail.get(email);
      if (window && now < window.end && window.count >= identityAttemptLimit) {
        return Math.ceil((window.end - now) / 1000);
      }
      if (serviceAttempts >= serviceAttemptLimit) return Math.ceil((serviceWindowEnd - now) / 1000);
      if (activeAttempts >= concurrentAttemptLimit) return 2;

      if (!window || now >= window.end) {
        window = {end: now + attemptWindow, count: 0};
        attemptsByEmail.set(email, window);
      }
      window.count++;
      serviceAttempts++;
      activeAttempts++;
      return 0;
    },
    release() { activeAttempts-- },
  };
}

export function createIdentity(store, clock) {
  const budget = createAttemptBudget();
  const dummyCredentials = {salt: randomBytes(16), passwordHash: randomBytes(hashLength)};

  return {
    async signIn({email, password}) {
      email = normalizeEmail(email);
      if (!email || !validPassword(password)) return {outcome: 'invalid'};

      const retryAfter = budget.reserve(email, clock());
      if (retryAfter) return {outcome: 'limited', retryAfter};
      try {
        const account = store.findCredentials(email);
        const {salt, passwordHash} = account || dummyCredentials;
        const actualHash = await derivePassword(password, salt);
        if (!timingSafeEqual(actualHash, passwordHash) || !account) return {outcome: 'invalid'};

        const {token, session} = prepareSession(clock());
        store.deleteExpiredSessions(session.createdAt);
        store.addSession({...session, accountId: account.id});
        return {outcome: 'accepted', token, accountId: account.id};
      } finally {
        budget.release();
      }
    },
    identify(token) {
      if (typeof token !== 'string' || !/^[A-Za-z0-9_-]{43}$/u.test(token)) return undefined;
      return store.findPrincipal(digestToken(token), clock());
    },
  };
}
