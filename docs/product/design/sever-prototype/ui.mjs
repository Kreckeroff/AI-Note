const paths = {
  north: 'M12 2l2.8 7.2L22 12l-7.2 2.8L12 22l-2.8-7.2L2 12l7.2-2.8L12 2z M12 9v6m-3-3h6',
  docs: 'M4 5a2 2 0 012-2h14v17H6a2 2 0 010-4h14 M4 5v13 M8 7h8m-8 4h6',
  graph: 'M7 16l5-9m2 1l5 7M8 18h9 M7 19a2 2 0 11-4 0 2 2 0 014 0zM15 5a2 2 0 11-4 0 2 2 0 014 0zM21 18a2 2 0 11-4 0 2 2 0 014 0z',
  flow: 'M4 4h6v5H4zm10 11h6v5h-6zm3-11h3v5h-3z M7 9v8h7m-4-11h7',
  board: 'M3 5h18v14H3z M9 5v14m6-14v14 M6 8v4m6-4v6m6-6v3',
  search: 'M20 20l-4-4 M18 11a7 7 0 11-14 0 7 7 0 0114 0z',
  plus: 'M12 5v14 M5 12h14',
  close: 'm6 6 12 12m0-12L6 18',
  chevron: 'm9 6 6 6-6 6',
  down: 'm6 9 6 6 6-6',
  folder: 'M3 7V5a1 1 0 011-1h5l2 2h9a1 1 0 011 1v12H3z',
  doc: 'M5 3h9l5 5v13H5z M14 3v6h5m-11 3h8m-8 4h7',
  shield: 'M12 22s7-3.4 7-10V5l-7-3-7 3v7c0 6.6 7 10 7 10z M9 12l2 2 4-4',
  home: 'm3 10 9-7 9 7v10H3zm6 10v-6h6v6',
  settings: 'M12 15a3 3 0 110-6 3 3 0 010 6z M12 2v4m0 12v4m10-10h-4M6 12H2m17-7-3 3m-8 8-3 3m0-14 3 3m8 8 3 3',
  sliders: 'M4 6h5m4 0h7M4 12h10m4 0h2M4 18h3m4 0h9M9 4v4m4-4v4m1 2v4m4-4v4M7 16v4m4-4v4',
  pen: 'M4 20l4-1 12-12-4-4L4 15zm10-15 4 4',
  share: 'M12 16V3m-4 4 4-4 4 4 M6 12H3v9h18v-9h-3',
  panel: 'M3 4h18v16H3z M15 4v16',
  side: 'M3 4h18v16H3z M9 4v16',
  check: 'm5 12 4 4L19 6',
  arrow: 'M4 12h15m-6-6 6 6-6 6',
  left: 'M20 12H5m6 6-6-6 6-6',
  outward: 'M7 17l10-10M9 7h8v8',
  link: 'm10 14 4-4m-6 3-2 2a3 3 0 004 4l3-3m3-5 2-2a3 3 0 00-4-4l-3 3',
  spark: 'M12 3l2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5L12 3z',
  book: 'M12 6C9 4 6 4 3 5v14c3-1 6-1 9 1 3-2 6-2 9-1V5c-3-1-6-1-9 1zm0 0v14',
  list: 'M9 6h11M9 12h11M9 18h11m-16-12h1m-1 6h1m-1 6h1',
  clock: 'M21 12a9 9 0 11-18 0 9 9 0 0118 0z M12 6v6l4 2',
  user: 'M12 12a4 4 0 100-8 4 4 0 000 8z M4 21a8 8 0 0116 0',
  eye: 'M2 12s4-6 10-6 10 6 10 6-4 6-10 6-10-6-10-6z M15 12a3 3 0 11-6 0 3 3 0 016 0z',
  play: 'M8 4l12 8-12 8z',
  code: 'm8 7-5 5 5 5m8-10 5 5-5 5m-3-13-2 16',
  sun: 'M12 16a4 4 0 110-8 4 4 0 010 8z M12 2v3m0 14v3m10-10h-3M5 12H2m17-7-2 2m-10 10-2 2m0-14 2 2m10 10 2 2',
  moon: 'M20 14a8 8 0 01-10-10 8 8 0 1010 10z',
  key: 'M16 12a5 5 0 100-10 5 5 0 000 10z m-3-1-8 8v3m0-3h4v-3h3',
  log: 'M5 12h14m-5-5 5 5-5 5 M9 4H4v16h5',
  lock: 'M5 10h14v11H5z m3 0V7a4 4 0 118 0v3',
  layers: 'm3 8 9-5 9 5-9 5-9-5zm0 5 9 5 9-5m-18 5 9 5 9-5',
  grip: 'M9 5h.01M15 5h.01M9 12h.01M15 12h.01M9 19h.01M15 19h.01',
  message: 'M3 4h18v13H8l-5 4z M7 8h10m-10 4h8',
  upload: 'M12 15V3m-4 4 4-4 4 4 M4 14v6h16v-6',
};

export function escapeHtml(value = '') {
  return String(value).replace(/[&<>"']/g, char => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;'}[char]));
}

export function icon(name, size = 16) {
  return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${paths[name]}"/></svg>`;
}

export function button(action, label, glyph, {className = '', value, title, pressed, submit = false, disabled = false} = {}) {
  const iconOnly = !label;
  const attributes = [
    action && `data-action="${escapeHtml(action)}"`,
    value !== undefined && `data-value="${escapeHtml(value)}"`,
    title && `title="${escapeHtml(title)}"`,
    iconOnly && `aria-label="${escapeHtml(title || '')}"`,
    pressed !== undefined && `aria-pressed="${Boolean(pressed)}"`,
    disabled && 'disabled',
  ].filter(Boolean).join(' ');
  return `<button type="${submit ? 'submit' : 'button'}" class="btn ${iconOnly ? 'icon ' : ''}${className}" ${attributes}>${glyph ? icon(glyph) : ''}${label ? escapeHtml(label) : ''}</button>`;
}

export function avatar(person, small = false) {
  return `<span class="avatar${small ? ' small' : ''}" data-tone="${person.tone || 'agent'}" aria-hidden="true">${escapeHtml(person.initials)}</span>`;
}

export function entity(action, value, glyph, name, description, tone = 'document') {
  return `<button type="button" class="entity" data-action="${action}" data-value="${escapeHtml(value)}" data-tone="${tone}">
    <span class="entity-icon">${icon(glyph,15)}</span><span class="entity-text"><strong>${escapeHtml(name)}</strong>${description ? `<span class="small">${escapeHtml(description)}</span>` : ''}</span>${icon('outward',13)}
  </button>`;
}

export function selectOptions(values, selected) {
  return Object.entries(values).map(([value, label]) =>
    `<option value="${escapeHtml(value)}"${value === selected ? ' selected' : ''}>${escapeHtml(label)}</option>`
  ).join('');
}
