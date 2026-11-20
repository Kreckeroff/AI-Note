export type SelectionAnchor = {
  revision: string;
  quote: string;
  start: number;
  end: number;
  before: string;
  after: string;
};

export type DiscussionAnchor = {
  state: 'attached' | 'missing' | 'ambiguous';
  quote: string;
  revision: string;
  start?: number;
  end?: number;
  historyTo?: string;
};

export function selectedPageAnchor(root: HTMLElement, selection: Selection | null, revision: string): SelectionAnchor | null {
  if (!selection || selection.isCollapsed || selection.rangeCount !== 1) return null;
  const range = selection.getRangeAt(0);
  if (!root.contains(range.startContainer) || !root.contains(range.endContainer)) return null;
  const prefix = document.createRange();
  prefix.selectNodeContents(root);
  prefix.setEnd(range.startContainer, range.startOffset);
  const start = Array.from(prefix.cloneContents().textContent ?? '').length;
  prefix.setEnd(range.endContainer, range.endOffset);
  const end = Array.from(prefix.cloneContents().textContent ?? '').length;
  const text = Array.from(root.textContent ?? '');
  const quote = text.slice(start, end).join('');
  if (!quote.trim() || end > 60_000) return null;
  return {
    revision, quote, start, end,
    before: text.slice(Math.max(0, start - 32), start).join(''),
    after: text.slice(end, end + 32).join(''),
  };
}

export function discussionRange(root: HTMLElement, anchor: DiscussionAnchor): Range | null {
  const {state, start, end, quote} = anchor;
  if (state !== 'attached' || !Number.isSafeInteger(start) || !Number.isSafeInteger(end) ||
    start === undefined || end === undefined || start < 0 || end <= start) return null;
  const nodes = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const range = document.createRange();
  let offset = 0;
  let started = false;
  for (let node = nodes.nextNode(); node; node = nodes.nextNode()) {
    const chars = Array.from(node.nodeValue ?? '');
    const next = offset + chars.length;
    if (!started && start < next) {
      range.setStart(node, chars.slice(0, start - offset).join('').length);
      started = true;
    }
    if (started && end <= next) {
      range.setEnd(node, chars.slice(0, end - offset).join('').length);
      return range.toString() === quote ? range : null;
    }
    offset = next;
  }
  return null;
}

export function scrollToRange(range: Range) {
  if (typeof Highlight === 'function' && CSS.highlights) {
    CSS.highlights.set('page-comment', new Highlight(range));
  } else {
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  }
  const rect = range.getBoundingClientRect();
  window.scrollBy({top: rect.top - window.innerHeight / 2});
}

export function focusLinkedFragment(root: HTMLElement, revision: string) {
  const link = /^#fragment-(\d{1,19})-(\d{1,10})-(\d{1,10})$/u.exec(window.location.hash);
  if (!link || link[1] !== revision) return;
  const start = Number(link[2]);
  const end = Number(link[3]);
  if (end <= start || end > 60_000) return;
  const quote = Array.from(root.textContent ?? '').slice(start, end).join('');
  if (!quote) return;
  const range = discussionRange(root, {state: 'attached', start, end, quote, revision});
  if (range) {
    root.focus({preventScroll: true});
    scrollToRange(range);
  }
}
