export const coverOptions = [
  {value: 'orbit', label: 'Орбита', description: 'Лиловая сфера и тонкие орбиты'},
  {value: 'aurora', label: 'Сияние', description: 'Бирюзовые световые нити'},
  {value: 'grid', label: 'Картография', description: 'Янтарное поле и линии рельефа'},
  {value: 'none', label: 'Без обложки', description: 'Страница начинается с заголовка'},
] as const;

export type CoverValue = (typeof coverOptions)[number]['value'] | `image:${string}`;
export type CoverView = {value: CoverValue; imageTo: string | null};
export type CoverChoice = 'draft' | 'published';
export type CoverDraft = {cover: CoverView | null; base: CoverValue | null; changed: boolean};

const uuid = '[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}';
const customValue = new RegExp(`^image:(${uuid})$`, 'u');
const uploadPath = new RegExp(`^/workspace/${uuid}/spaces/${uuid}/pages/${uuid}/cover$`, 'u');

export function isCoverValue(value: unknown): value is CoverValue {
  return coverOptions.some(option => option.value === value) || typeof value === 'string' && customValue.test(value);
}

export function isCoverUploadPath(path: unknown): path is string {
  return typeof path === 'string' && uploadPath.test(path);
}

export function isCoverView(value: unknown): value is CoverView {
  if (value === null || typeof value !== 'object' || !('value' in value) || !('imageTo' in value) || !isCoverValue(value.value)) return false;
  const custom = customValue.exec(value.value);
  if (!custom) return value.imageTo === null;
  const suffix = `/${custom[1]}`;
  return typeof value.imageTo === 'string' && value.imageTo.endsWith(suffix) &&
    isCoverUploadPath(value.imageTo.slice(0, -suffix.length));
}

export function uploadedCover(value: unknown, endpoint: string): CoverView | undefined {
  if (!isCoverView(value) || value.imageTo === null || value.imageTo.slice(0, value.imageTo.lastIndexOf('/')) !== endpoint) return;
  return value;
}

export function chooseCoverDraft(draft: CoverDraft, selected: CoverView, published: CoverView | null | undefined): CoverDraft {
  const base = published?.value ?? draft.base;
  return {cover: selected, base, changed: selected.value !== base};
}

export function resolveCoverDraft(draft: CoverDraft, published: CoverView | null | undefined, choice: CoverChoice | null, rebase: boolean) {
  const sameAsPublished = draft.cover?.value === published?.value;
  const requiresChoice = Boolean(published && published.value !== draft.base && draft.changed && !sameAsPublished);
  const choosesPublished = requiresChoice && choice === 'published';
  const keepsPublished = published && (choosesPublished || rebase && (!draft.changed || sameAsPublished));
  return {
    preview: keepsPublished ? published : draft.cover,
    cover: draft.changed && !keepsPublished ? draft.cover?.value ?? null : null,
    base: rebase && published ? published.value : draft.base,
    requiresChoice,
  };
}
