export type PageAttachment = {id: string; name: string; bytes: number; downloadTo: string | null; imageTo: string | null};

export type AttachmentDraft = {
  files: readonly PageAttachment[];
  added: readonly string[];
  removed: readonly string[];
};

export const maxAttachmentSize = 20 * 1024 * 1024;
export const maxAttachmentCount = 20;
export const maxAttachmentTotal = 100 * 1024 * 1024;

const uuid = '[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}';
const uuidPattern = new RegExp(`^${uuid}$`, 'u');
const pagePath = `/workspace/${uuid}/spaces/${uuid}/pages/${uuid}`;
const uploadPath = new RegExp(`^(${pagePath})/attachments/drafts/(${uuid})$`, 'u');
const publishedPath = new RegExp(`^${pagePath}/attachments/${uuid}$`, 'u');
const draftImagePath = new RegExp(`^${pagePath}/attachments/drafts/${uuid}/(${uuid})/image$`, 'u');

export function isAttachmentId(value: unknown): value is string {
  return typeof value === 'string' && uuidPattern.test(value);
}

export function isAttachmentUploadPath(path: unknown, draft: unknown, action: unknown): path is string {
  if (typeof path !== 'string' || typeof action !== 'string') return false;
  const match = uploadPath.exec(path);
  return match !== null && match[2] === draft && action.split('?')[0] === `${match[1]}/edit`;
}

export function isPageAttachment(value: unknown): value is PageAttachment {
  if (value === null || typeof value !== 'object' || !('id' in value) || !isAttachmentId(value.id) ||
      !('name' in value) || typeof value.name !== 'string' || !value.name ||
      !('bytes' in value) || typeof value.bytes !== 'number' || !Number.isInteger(value.bytes) ||
      value.bytes <= 0 || value.bytes > maxAttachmentSize || !('downloadTo' in value) || !('imageTo' in value)) return false;
  if (typeof value.downloadTo === 'string' && publishedPath.test(value.downloadTo)) {
    return value.imageTo === null || value.imageTo === `${value.downloadTo}/image`;
  }
  return value.downloadTo === null && (value.imageTo === null ||
    typeof value.imageTo === 'string' && draftImagePath.exec(value.imageTo)?.[1] === value.id);
}

export function uploadedAttachment(value: unknown, endpoint: string, id: string): PageAttachment | undefined {
  if (isPageAttachment(value) && value.id === id && value.downloadTo === null &&
      (value.imageTo === null || value.imageTo === `${endpoint}/${id}/image`)) return value;
}

export function addDraftAttachment(draft: AttachmentDraft, file: PageAttachment): AttachmentDraft {
  if (draft.files.some(existing => existing.id === file.id)) return draft;
  return {...draft, files: [...draft.files, file], added: [...new Set([...draft.added, file.id])]};
}

export function removeDraftAttachment(draft: AttachmentDraft, id: string): AttachmentDraft {
  const staged = draft.added.includes(id);
  if (!staged && !draft.files.some(file => file.id === id)) return draft;
  return {
    files: draft.files.filter(file => file.id !== id),
    added: staged ? draft.added.filter(value => value !== id) : draft.added,
    removed: staged ? draft.removed : [...draft.removed, id],
  };
}
