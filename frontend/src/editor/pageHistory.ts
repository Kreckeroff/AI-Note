export type PageHistoryEntry = {
  revision: string;
  author: string | null;
  recordedAt: string | null;
  href: string;
};

export type PageHistory = {
  currentRevision: string;
  items: PageHistoryEntry[];
  nextTo: string | null;
};

const uuid = '[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}';
const historyPath = new RegExp(`^/workspace/${uuid}/spaces/${uuid}/pages/${uuid}/revisions$`, 'u');
const maxRevision = 9223372036854775807n;

export function isPageHistoryPath(path: unknown): path is string {
  return typeof path === 'string' && historyPath.test(path);
}

function isRevision(value: unknown): value is string {
  return typeof value === 'string' && /^[1-9][0-9]{0,18}$/u.test(value) && BigInt(value) <= maxRevision;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object';
}

function isEntry(value: unknown, historyTo: string): value is PageHistoryEntry {
  return isRecord(value) && isRevision(value.revision) && value.href === `${historyTo}/${value.revision}` &&
    (value.author === null || typeof value.author === 'string') &&
    (value.recordedAt === null || typeof value.recordedAt === 'string' && Number.isFinite(Date.parse(value.recordedAt)));
}

export function readPageHistory(value: unknown, historyTo: string): PageHistory | undefined {
  if (!isPageHistoryPath(historyTo) || !isRecord(value) || !isRevision(value.currentRevision) ||
      !Array.isArray(value.items) || !value.items.every(item => isEntry(item, historyTo))) return;

  const currentRevision = BigInt(value.currentRevision);
  const ordered = value.items.every((item, index, items) => {
    const number = BigInt(item.revision);
    return index === 0 ? number <= currentRevision : number < BigInt(items[index - 1].revision);
  });
  if (!ordered) return;

  const last = value.items.at(-1);
  if (value.nextTo !== null && (!last || value.nextTo !== `${historyTo}?before=${last.revision}`)) return;
  return {currentRevision: value.currentRevision, items: value.items, nextTo: value.nextTo};
}

export function historySnapshotHref(href: string, editing: boolean): string {
  return editing ? `${href}?preview=1` : href;
}
