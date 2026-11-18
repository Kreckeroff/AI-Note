const collator = new Intl.Collator('ru', {sensitivity: 'base', numeric: true});
const pageSize = 20;
const fold = value => value.normalize('NFC').toLowerCase().normalize('NFC');

export function sortSpaces(spaces) {
  return spaces.toSorted((left, right) => collator.compare(left.name, right.name) ||
    collator.compare(left.ownerName, right.ownerName) || left.id.localeCompare(right.id));
}

export function catalogPage(spaces, {q, scope, page}) {
  const search = fold(q);
  const matches = spaces.filter(space => {
    if (scope === 'mine' && !space.isOwner) return false;
    if (scope === 'membership' && !space.grantRole) return false;
    return !search || fold(space.name).includes(search);
  });
  const total = matches.length;
  const pages = Math.max(1, Math.ceil(total / pageSize));
  page = Math.min(page, pages);
  const offset = (page - 1) * pageSize;
  const rows = matches.slice(offset, offset + pageSize);
  return {rows, options: {q, scope, page}, total, pages, start: offset + 1, end: offset + rows.length};
}
