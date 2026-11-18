import type {CatalogOptions} from './model';

export function spacesPath(tenantId: string): string {
  return `/workspace/${tenantId}/spaces`;
}

export function spacePath(tenantId: string, spaceId: string): string {
  return `${spacesPath(tenantId)}/${spaceId}`;
}

export function newSpacePath(tenantId: string): string {
  return `${spacesPath(tenantId)}/new`;
}

export function sectionPath(tenantId: string, spaceId: string, sectionId: string): string {
  return `${spacePath(tenantId, spaceId)}/sections/${sectionId}`;
}

export function pagePath(tenantId: string, spaceId: string, pageId: string): string {
  return `${spacePath(tenantId, spaceId)}/pages/${pageId}`;
}

export function newPagePath(tenantId: string, spaceId: string, sectionId: string): string {
  return `${sectionPath(tenantId, spaceId, sectionId)}/pages/new`;
}

export function newSectionPath(tenantId: string, spaceId: string, options: CatalogOptions, from?: string): string {
  const path = withOptions(sectionPath(tenantId, spaceId, 'new'), options);
  return from ? `${path}${path.includes('?') ? '&' : '?'}from=${encodeURIComponent(from)}` : path;
}

export function withOptions(path: string, {q, page, scope}: CatalogOptions): string {
  const params = new URLSearchParams();
  if (q) params.set('q', q);
  if (scope !== 'all') params.set('scope', scope);
  if (page > 1) params.set('page', String(page));
  return params.size ? `${path}?${params}` : path;
}
