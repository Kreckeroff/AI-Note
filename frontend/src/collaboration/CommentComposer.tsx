import {useId, useLayoutEffect, useRef, useState, type FormEvent} from 'react';
import {queryTo, useCollaborationQuery} from './api';
import type {SelectionAnchor} from './pageAnchors';

type Mention = {id: string; name: string};
export type CommentDraft = {body: string; mentions: Mention[]; submitted?: {payload: string; id: string}};
export const emptyComment: CommentDraft = {body: '', mentions: []};

const maxCommentLength = 500;

export function CommentComposer({discussionTo, draft, onChange, onSend, anchor, onClearAnchor, reply = false, busy}: {
  discussionTo: string;
  draft: CommentDraft;
  onChange(draft: CommentDraft): void;
  onSend(event: FormEvent<HTMLFormElement>): void;
  anchor?: SelectionAnchor | null;
  onClearAnchor?(): void;
  reply?: boolean;
  busy: boolean;
}) {
  const id = useId();
  const editor = useRef<HTMLTextAreaElement>(null);
  const selectionAfterInsert = useRef<number | null>(null);
  const [caret, setCaret] = useState(0);
  const [suggesting, setSuggesting] = useState(false);
  const marker = suggesting ? /(?:^|\s)@([^@\r\n]{0,70})$/u.exec(draft.body.slice(0, caret)) : null;
  const result = useCollaborationQuery<{people: Mention[]}>(
    marker ? queryTo(discussionTo, {mentions: marker[1] || ' '}) : null,
  );
  const count = Array.from(draft.body.trim()).length;
  const label = reply ? 'Ответ в ветке' : 'Новый комментарий';

  useLayoutEffect(() => {
    if (anchor || reply) editor.current?.focus({preventScroll: true});
  }, [anchor, reply]);

  useLayoutEffect(() => {
    const next = selectionAfterInsert.current;
    if (next === null) return;
    editor.current?.focus({preventScroll: true});
    editor.current?.setSelectionRange(next, next);
    selectionAfterInsert.current = null;
  }, [draft.body]);

  function choose(person: Mention) {
    if (!marker) return;
    const start = caret - marker[1].length - 1;
    const replacement = `@${person.name} `;
    const body = draft.body.slice(0, start) + replacement + draft.body.slice(caret);
    const mentions = draft.mentions.filter(item => item.id !== person.id && body.includes('@' + item.name));
    onChange({body, mentions: [...mentions, person]});
    selectionAfterInsert.current = start + replacement.length;
    setCaret(start + replacement.length);
    setSuggesting(false);
  }

  return <form className="comment-compose" onSubmit={onSend}>
    {anchor && <div className="comment-anchor">
      <p>{anchor.quote}</p>
      <button type="button" className="space-action space-secondary" onClick={onClearAnchor}>Убрать привязку</button>
    </div>}
    <label htmlFor={id}>{label}</label>
    <textarea ref={editor} id={id} rows={3} value={draft.body} maxLength={maxCommentLength * 2} required
      readOnly={busy} aria-describedby={`${id}-length`} aria-invalid={count > maxCommentLength || undefined}
      placeholder={reply ? 'Ответить по теме' : 'Напишите комментарий. @ — упомянуть участника'}
      onChange={event => {
        const {value, selectionStart} = event.currentTarget;
        setCaret(selectionStart);
        setSuggesting(true);
        onChange({body: value, mentions: draft.mentions.filter(item => value.includes('@' + item.name))});
      }}
      onSelect={event => setCaret(event.currentTarget.selectionStart)}
      onKeyDown={event => {
        if (event.key === 'Escape' && marker) {
          event.stopPropagation();
          event.preventDefault();
          setSuggesting(false);
        } else if (event.key === 'ArrowDown' && result?.data?.people.length) {
          event.preventDefault();
          document.getElementById(`${id}-mention-0`)?.focus();
        }
      }} />
    {marker && result?.error && <p className="collaboration-hint" role="status">{result.error}</p>}
    {marker && result?.data?.people.length ? <ul className="comment-mentions" aria-label="Упомянуть участника">
      {result.data.people.map((person, index) => <li key={person.id}>
        <button id={`${id}-mention-${index}`} type="button" className="space-action space-secondary" onClick={() => choose(person)}
          onKeyDown={event => {
            if (event.key === 'Escape' || event.key === 'ArrowUp' && !index) {
              event.stopPropagation();
              event.preventDefault();
              if (event.key === 'Escape') setSuggesting(false);
              editor.current?.focus({preventScroll: true});
            } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
              event.preventDefault();
              const next = Math.min(result.data!.people.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1));
              document.getElementById(`${id}-mention-${next}`)?.focus();
            }
          }}>{person.name}</button>
      </li>)}
    </ul> : null}
    <div className="comment-compose-foot">
      <span className={count > maxCommentLength ? 'form-error' : 'collaboration-hint'} id={`${id}-length`}>
        {count} / {maxCommentLength}
      </span>
      <button type="submit" className="space-action primary-action" disabled={busy || count < 1 || count > maxCommentLength}>
        Отправить
      </button>
    </div>
  </form>;
}
