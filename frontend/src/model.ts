import {isCoverUploadPath, isCoverValue, isCoverView, type CoverValue, type CoverView} from './editor/pageCover';
import {isPageHistoryPath} from './editor/pageHistory';

export interface TenantLink {
  id: string;
  name: string;
}

export interface Tenant extends TenantLink {
  role: 'member' | 'admin' | 'superadmin';
  canCreate: boolean;
}

export interface Space {
  id: string;
  name: string;
  ownerName: string;
  isOwner: boolean;
  tenantRole: Tenant['role'];
  grantRole: 'viewer' | 'editor' | null;
}

export interface CatalogOptions {
  q: string;
  scope: 'all' | 'mine' | 'membership';
  page: number;
}

export interface CatalogModel {
  options: CatalogOptions;
  rows: Space[];
  total: number;
  pages: number;
  start: number;
  end: number;
}

export interface WorkspaceModel {
  tenant: Tenant;
  tenants: TenantLink[];
  accountName: string;
  availableSpaces: Space[];
  catalog: CatalogModel;
}

export interface Section {
  id: string;
  title: string;
}

export interface DocumentPage {
  id: string;
  sectionId: string;
  title: string;
}

export interface DocumentFormModel {
  operationId: string;
  title: string;
  error: string;
  invalidName: boolean;
  cancelTo: string;
  action: string;
  retryTo: string | null;
}

export interface PageReadingModel {
  text: string;
  editTo: string | null;
  renderedHtml: string | null;
  cover?: CoverView | null;
  historyTo?: string | null;
}

interface PageRevision {
  text: string;
  revision: string;
  cover?: CoverView | null;
}

export interface PageEditorForm extends PageRevision {
  action: string;
  cancelTo: string | null;
  source: string;
  contentFormat: 'literal' | 'markdown';
  editorMode: 'visual' | 'source';
  error: string | null;
  conflict: PageRevision | null;
  canSave: boolean;
  coverBase?: CoverValue | null;
  coverChanged?: boolean;
  coverUploadTo?: string | null;
  historyTo?: string | null;
  textBase?: string | null;
  textUnchanged?: boolean;
}

export interface SectionTreeModel {
  sections: Section[];
  selected: Section | null;
  canCreateSection: boolean;
  form: DocumentFormModel | null;
  pages: DocumentPage[];
  selectedPage: DocumentPage | null;
  canCreatePage: boolean;
  pageForm: DocumentFormModel | null;
  reading: PageReadingModel | null;
  editor: PageEditorForm | null;
}

export type AccountPageModel =
  | {kind: 'signIn'; email: string; error: string; registrationEnabled: boolean; returnTo: string}
  | {kind: 'registrationStart'; email: string; error: string}
  | {kind: 'registrationRequested'}
  | {kind: 'registrationComplete'; token: string; name: string; error: string}
  | {kind: 'noTenant'; accountName: string; tenants: TenantLink[]};

export type CreationPageModel = {
  kind: 'createSpace';
  workspace: WorkspaceModel;
  operationId: string;
  name: string;
  error: string;
  invalidName: boolean;
  newForm: boolean;
};

export type WorkspacePageModel =
  | {kind: 'catalog'; workspace: WorkspaceModel}
  | {kind: 'catalogUnavailable'; retryTo: string}
  | {kind: 'documentsUnavailable'; retryTo: string}
  | {kind: 'pageEditRetry'; editor: PageEditorForm}
  | {kind: 'closedSpace'; workspace: WorkspaceModel; space: Space; tree: SectionTreeModel}
  | {kind: 'spaceNotFound'}
  | {kind: 'sectionNotFound'}
  | {kind: 'pageNotFound'}
  | CreationPageModel;

export type PageModel = AccountPageModel | WorkspacePageModel;

type Fields = Record<string, unknown>;

function fields(value: unknown): value is Fields {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function strings(value: Fields, ...keys: string[]): boolean {
  return keys.every(key => typeof value[key] === 'string');
}

function hasUuid(value: unknown): boolean {
  return typeof value === 'string' && /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/u.test(value);
}

function isRole(value: unknown): boolean {
  return value === 'member' || value === 'admin' || value === 'superadmin';
}

function isTenantLink(value: unknown): value is Fields {
  return fields(value) && hasUuid(value.id) && strings(value, 'name');
}

function isSpace(value: unknown): boolean {
  return fields(value) && hasUuid(value.id) && strings(value, 'name', 'ownerName') &&
    typeof value.isOwner === 'boolean' && isRole(value.tenantRole) &&
    (value.grantRole === null || value.grantRole === 'viewer' || value.grantRole === 'editor');
}

function isList(value: unknown, predicate: (item: unknown) => boolean): boolean {
  return Array.isArray(value) && value.every(predicate);
}

function isCount(value: unknown): boolean {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}

function isCatalog(value: unknown): boolean {
  if (!fields(value) || !fields(value.options)) return false;
  const {q, scope, page} = value.options;
  return typeof q === 'string' && isCount(page) &&
    (scope === 'all' || scope === 'mine' || scope === 'membership') &&
    isList(value.rows, isSpace) && [value.total, value.pages, value.start, value.end].every(isCount);
}

function isWorkspace(value: unknown): boolean {
  return fields(value) && isTenantLink(value.tenant) && isRole(value.tenant.role) &&
    typeof value.tenant.canCreate === 'boolean' && strings(value, 'accountName') &&
    isList(value.tenants, isTenantLink) && isList(value.availableSpaces, isSpace) && isCatalog(value.catalog);
}

function isSection(value: unknown): boolean {
  return fields(value) && hasUuid(value.id) && strings(value, 'title');
}

function isDocumentPage(value: unknown): boolean {
  return fields(value) && hasUuid(value.id) && hasUuid(value.sectionId) && strings(value, 'title');
}

function isWorkspacePath(value: unknown): boolean {
  return typeof value === 'string' && value.startsWith('/workspace/');
}

function isDocumentForm(value: unknown): boolean {
  return fields(value) && hasUuid(value.operationId) && strings(value, 'title', 'error') &&
    typeof value.invalidName === 'boolean' && isWorkspacePath(value.cancelTo) &&
    isWorkspacePath(value.action) && (value.retryTo === null || isWorkspacePath(value.retryTo));
}

function isPageReading(value: unknown): boolean {
  return fields(value) && strings(value, 'text') &&
    (value.renderedHtml === null || typeof value.renderedHtml === 'string') &&
    (value.editTo === null || isWorkspacePath(value.editTo)) && isOptionalCover(value.cover) &&
    (value.historyTo == null || isPageHistoryPath(value.historyTo));
}

function isPageRevision(value: unknown): value is Fields {
  return fields(value) && strings(value, 'text', 'revision') && isOptionalCover(value.cover);
}

function isOptionalCover(value: unknown): boolean {
  return value == null || isCoverView(value);
}

function isPageEditorForm(value: unknown): boolean {
  return isPageRevision(value) && isWorkspacePath(value.action) && strings(value, 'source') &&
    (value.contentFormat === 'literal' || value.contentFormat === 'markdown') &&
    (value.editorMode === 'visual' || value.editorMode === 'source') &&
    (value.cancelTo === null || isWorkspacePath(value.cancelTo)) &&
    (value.error === null || typeof value.error === 'string') &&
    (value.conflict === null || isPageRevision(value.conflict)) && typeof value.canSave === 'boolean' &&
    (value.coverBase == null || isCoverValue(value.coverBase)) &&
    (value.coverUploadTo == null || isCoverUploadPath(value.coverUploadTo)) &&
    (value.historyTo == null || isPageHistoryPath(value.historyTo)) &&
    (value.coverChanged === undefined || typeof value.coverChanged === 'boolean') &&
    (value.textBase == null || typeof value.textBase === 'string' && /^[a-f0-9]{64}$/u.test(value.textBase)) &&
    (value.textUnchanged === undefined || typeof value.textUnchanged === 'boolean');
}

function isSectionTree(value: unknown): boolean {
  if (!fields(value)) return false;
  const hasSelectedSection = isSection(value.selected);
  const hasSelectedPage = hasSelectedSection && isDocumentPage(value.selectedPage);
  return isList(value.sections, isSection) && (value.selected === null || hasSelectedSection) &&
    typeof value.canCreateSection === 'boolean' && (value.form === null || value.canCreateSection && isDocumentForm(value.form)) &&
    isList(value.pages, isDocumentPage) && (value.selectedPage === null || hasSelectedPage) &&
    typeof value.canCreatePage === 'boolean' &&
    (value.pageForm === null || hasSelectedSection && value.canCreatePage && isDocumentForm(value.pageForm)) &&
    (value.reading === null || hasSelectedPage && isPageReading(value.reading)) &&
    (value.editor === null || hasSelectedPage && isPageEditorForm(value.editor));
}

function isPage(value: unknown): value is PageModel {
  if (!fields(value)) return false;
  switch (value.kind) {
    case 'signIn':
      return strings(value, 'email', 'error', 'returnTo') && typeof value.registrationEnabled === 'boolean';
    case 'registrationStart':
      return strings(value, 'email', 'error');
    case 'registrationRequested':
    case 'spaceNotFound':
    case 'sectionNotFound':
    case 'pageNotFound':
      return true;
    case 'registrationComplete':
      return strings(value, 'token', 'name', 'error');
    case 'noTenant':
      return strings(value, 'accountName') && isList(value.tenants, isTenantLink);
    case 'catalog':
      return isWorkspace(value.workspace);
    case 'catalogUnavailable':
    case 'documentsUnavailable':
      return strings(value, 'retryTo');
    case 'pageEditRetry':
      return isPageEditorForm(value.editor);
    case 'closedSpace':
      return isWorkspace(value.workspace) && isSpace(value.space) && isSectionTree(value.tree);
    case 'createSpace':
      return isWorkspace(value.workspace) && hasUuid(value.operationId) && strings(value, 'name', 'error') &&
        typeof value.invalidName === 'boolean' && typeof value.newForm === 'boolean';
    default:
      return false;
  }
}

export function readPage(source: string): PageModel | undefined {
  try {
    const parsed: unknown = JSON.parse(source);
    return isPage(parsed) ? parsed : undefined;
  } catch {
    // The browser can still use the original HTML forms and links.
  }
}
