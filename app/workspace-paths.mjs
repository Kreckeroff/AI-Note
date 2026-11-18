const uuid = '[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}';
const uuidPattern = new RegExp(`^${uuid}$`, 'u');
const spacePath = new RegExp(`^/workspace/(${uuid})/spaces(?:/(new|${uuid}))?$`, 'u');

export const workspacePath = '/workspace';
export const isWorkspaceId = value => typeof value === 'string' && value.length === 36 && uuidPattern.test(value);
export const parseSpacePath = path => {
  const match = spacePath.exec(path);
  return match?.[0] === path ? match : null;
};
export const spacesPath = tenantId => `${workspacePath}/${tenantId}/spaces`;
export const newSpacePath = tenantId => `${spacesPath(tenantId)}/new`;
export const spacePagePath = (tenantId, spaceId) => `${spacesPath(tenantId)}/${spaceId}`;

export function catalogOptions(query = new URLSearchParams()) {
  const q = Array.from((query.get('q') ?? '').trim()).slice(0, 100).join('');
  const requestedScope = query.get('scope');
  const scope = ['mine', 'membership'].includes(requestedScope) ? requestedScope : 'all';
  const requestedPage = query.get('page') ?? '';
  const page = /^\d{1,15}$/u.test(requestedPage) ? Math.max(1, Number(requestedPage)) : 1;
  return {q, scope, page};
}

export function withCatalogOptions(path, {q = '', scope = 'all', page = 1} = {}) {
  const query = new URLSearchParams();
  if (q) query.set('q', q);
  if (scope !== 'all') query.set('scope', scope);
  if (page > 1) query.set('page', String(page));
  return query.size ? `${path}?${query}` : path;
}

export function localWorkspaceTarget(value) {
  if (typeof value !== 'string' || !value) return '';
  const separator = value.indexOf('?');
  const path = separator === -1 ? value : value.slice(0, separator);
  const query = separator === -1 ? '' : value.slice(separator + 1);
  let canonical = workspacePath;
  if (path !== workspacePath) {
    const route = parseSpacePath(path);
    if (!route) return '';
    const [, tenantId, target] = route;
    canonical = target ? spacePagePath(tenantId, target) : spacesPath(tenantId);
  }
  const destination = withCatalogOptions(canonical, catalogOptions(new URLSearchParams(query)));
  return destination.length <= 1400 ? destination : '';
}
