import {createHash, randomBytes} from 'node:crypto';
import {AccountInputError, normalizeEmail, prepareAccount, prepareSession} from './identity.mjs';

const confirmationLifetimeMinutes = 30;
const confirmationLifetime = confirmationLifetimeMinutes * 60 * 1000;
const sendWindow = 60 * 1000;
const sendLimit = 24;
const pendingLimit = 200;
const tokenPattern = /^[A-Za-z0-9_-]{43}$/u;
const digest = token => createHash('sha256').update(token).digest('hex');

export function createRegistration({store, deliver, clock, record}) {
  const pending = new Map();
  const recipientCooldowns = new Map();
  let windowEnd = 0;
  let sends = 0;
  let completing = false;

  function findPending(token) {
    if (!tokenPattern.test(token)) return undefined;
    const confirmation = pending.get(digest(token));
    if (confirmation?.expiresAt > clock()) return confirmation;
  }

  function clearRecipient(email) {
    for (const [key, confirmation] of pending) {
      if (confirmation.email === email) pending.delete(key);
    }
  }

  return {
    request(enteredEmail, makeConfirmationLink) {
      const email = normalizeEmail(enteredEmail);
      if (!email) return 'invalid-email';
      const now = clock();
      for (const [key, confirmation] of pending) {
        if (confirmation.expiresAt <= now) pending.delete(key);
      }
      for (const [address, allowedAt] of recipientCooldowns) {
        if (allowedAt <= now) recipientCooldowns.delete(address);
      }
      if (now >= windowEnd) {
        windowEnd = now + sendWindow;
        sends = 0;
      }
      if (pending.size >= pendingLimit || sends >= sendLimit || recipientCooldowns.has(email)) {
        record('registration.requested', {outcome: 'limited'});
        return 'accepted';
      }
      // Apply the same request budget to any valid address. Dispatch separately from the response.
      sends++;
      recipientCooldowns.set(email, now + sendWindow);
      if (store.hasAccount(email)) {
        record('registration.requested', {outcome: 'accepted'});
        return 'accepted';
      }

      const token = randomBytes(32).toString('base64url');
      const key = digest(token);
      const confirmation = {email, expiresAt: now + confirmationLifetime, completing: false};
      pending.set(key, confirmation);
      const link = makeConfirmationLink(token);
      setImmediate(async () => {
        try {
          await deliver({email, link, expiresInMinutes: confirmationLifetimeMinutes});
          record('registration.delivery', {outcome: 'accepted'});
        } catch {
          pending.delete(key);
          record('registration.delivery', {outcome: 'failed'});
        }
      });
      return 'accepted';
    },
    isPending(token) {
      return Boolean(findPending(token));
    },
    async confirm({token, name, password}) {
      const confirmation = findPending(token);
      if (!confirmation || confirmation.completing) return {outcome: 'invalid-link'};
      if (completing) return {outcome: 'limited'};
      completing = true;
      confirmation.completing = true;
      try {
        const account = await prepareAccount({email: confirmation.email, name, password});
        if (findPending(token) !== confirmation) return {outcome: 'invalid-link'};
        const prepared = prepareSession(clock());
        const tenantId = store.createAccountWorkspaceAndSession({
          account,
          session: prepared.session,
          tenantName: 'Моя среда',
          membership: {role: 'superadmin', status: 'active', canCreate: true, current: true},
        });
        clearRecipient(confirmation.email);
        if (!tenantId) return {outcome: 'invalid-link'};
        return {outcome: 'accepted', accountId: account.id, tenantId, token: prepared.token};
      } catch (error) {
        if (!(error instanceof AccountInputError)) throw error;
        return {outcome: 'invalid-fields', message: error.message};
      } finally {
        confirmation.completing = false;
        completing = false;
      }
    },
  };
}
