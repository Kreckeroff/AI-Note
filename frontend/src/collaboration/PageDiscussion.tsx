import {useEffect, useRef, useState, type FormEvent} from 'react';
import {newOperationId, useCollaborationQuery, usePageCommand, queryTo} from './api';
import {CommentComposer, emptyComment, type CommentDraft} from './CommentComposer';
import type {DiscussionAnchor, SelectionAnchor} from './pageAnchors';

type ThreadStatus = 'active' | 'resolved';
type Reply = {id: string; body: string; author: string; createdAt: number};
type Thread = Reply & {
  status: ThreadStatus;
  version: number;
  canChange: boolean;
  anchor?: DiscussionAnchor | null;
  changedAt?: number | null;
  changedBy?: string | null;
  replies: Reply[];
  repliesOffset?: number;
  repliesNextOffset?: number | null;
};
type Discussion = {
  threads: Thread[];
  nextOffset: number | null;
  canComment: boolean;
};

const date = new Intl.DateTimeFormat('ru-RU', {dateStyle: 'medium', timeStyle: 'short'});

function CommentTime({stamp}: {stamp: number}) {
  const value = new Date(stamp);
  return Number.isFinite(value.getTime()) ? <time dateTime={value.toISOString()}>{date.format(value)}</time> : null;
}

function CopyCommentLink({href}: {href: string}) {
  const [copied, setCopied] = useState(false);
  return <a className="comment-address" href={href} onClick={async event => {
    if (!navigator.clipboard?.writeText || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    try {
      await navigator.clipboard.writeText(new URL(href, window.location.origin).href);
      setCopied(true);
    } catch { window.location.assign(href); }
  }}>{copied ? 'Ссылка скопирована' : 'Ссылка'}</a>;
}

function threadHref(pageTo: string, thread: string, comment = thread): string {
  const reply = comment === thread ? '' : `&comment=${comment}`;
  return `${pageTo}?thread=${thread}${reply}#comment-${comment}`;
}

function AddressedComment({pageTo, thread, comment}: {pageTo: string; thread: string; comment: Reply}) {
  return <>
    <div className="comment-meta">
      <strong>{comment.author}</strong><CommentTime stamp={comment.createdAt} />
      <CopyCommentLink href={threadHref(pageTo, thread, comment.id)} />
    </div>
    <p className="comment-body">{comment.body}</p>
  </>;
}

function AnchorQuote({pageTo, threadId, anchor, editing, onJump}: {
  pageTo: string;
  threadId: string;
  anchor: DiscussionAnchor;
  editing: boolean;
  onJump(anchor: DiscussionAnchor): boolean;
}) {
  const [unavailable, setUnavailable] = useState(false);
  const archived = /^[1-9][0-9]{0,18}$/u.test(anchor.revision);
  const historyTo = `${pageTo}/revisions/${anchor.revision}?comment=${threadId}`;
  const archiveQuote = archived && anchor.historyTo === historyTo + '#history-quote'
    ? historyTo + (editing ? '&preview=1' : '') + '#history-quote' : undefined;
  return <div className="comment-anchor">
    {anchor.state === 'attached' && anchor.quote && !unavailable ? <button type="button" className="comment-quote" onClick={() => {
      if (!onJump(anchor)) setUnavailable(true);
    }} aria-label={`К фрагменту · ${anchor.quote}`}>{anchor.quote}</button> : <>
      {anchor.quote && <p>{anchor.quote}</p>}
      <small className="collaboration-hint">{unavailable ? 'Обновите страницу, чтобы перейти к фрагменту.'
        : anchor.state === 'ambiguous' ? 'У цитаты несколько совпадений.' : 'Фрагмент изменён или удалён.'}</small>
      {archiveQuote && <a href={archiveQuote}
        target={editing ? '_blank' : undefined} rel={editing ? 'noopener' : undefined}>Цитата в прежней редакции</a>}
    </>}
  </div>;
}

export function PageDiscussion({pageTo, active, anchor, onClearAnchor, onJump, editing, reference}: {
  pageTo: string;
  active: boolean;
  anchor: SelectionAnchor | null;
  onClearAnchor(): void;
  onJump(anchor: DiscussionAnchor): boolean;
  editing: boolean;
  reference?: {thread: string; comment: string};
}) {
  const path = `${pageTo}/discussion`;
  const [status, setStatus] = useState<ThreadStatus>('active');
  const [offsets, setOffsets] = useState([0]);
  const [change, setChange] = useState(0);
  const [threadId, setThreadId] = useState(reference?.thread ?? '');
  const [replyFocus, setReplyFocus] = useState(reference?.comment ?? '');
  const [repliesOffset, setRepliesOffset] = useState(0);
  const [replyTo, setReplyTo] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, CommentDraft>>({});
  const focusedReference = useRef(false);
  const offset = offsets.at(-1)!;
  const href = threadId
    ? queryTo(path, replyFocus && replyFocus !== threadId
      ? {thread: threadId, reply: replyFocus} : {thread: threadId, repliesOffset})
    : queryTo(path, {status, offset});
  const result = useCollaborationQuery<Discussion>(href, change, active);
  const command = usePageCommand(path, () => setChange(current => current + 1));

  useEffect(() => {
    if (!active || !reference || focusedReference.current || !result?.data) return;
    const element = document.getElementById(`comment-${reference.comment}`);
    focusedReference.current = true;
    if (element) {
      element.scrollIntoView({block: 'nearest'});
      element.focus({preventScroll: true});
    }
  }, [active, reference, result]);

  useEffect(() => {
    if (!anchor) return;
    setStatus('active');
    setOffsets([0]);
    setThreadId('');
    setReplyTo(null);
    setReplyFocus('');
    setRepliesOffset(0);
  }, [anchor]);

  function selectStatus(value: ThreadStatus) {
    setStatus(value);
    setOffsets([0]);
    setThreadId('');
    setReplyTo(null);
    setReplyFocus('');
    setRepliesOffset(0);
    command.clearError();
  }

  function saveDraft(key: string, draft: CommentDraft) {
    setDrafts(values => ({...values, [key]: draft}));
  }

  async function sendComment(event: FormEvent<HTMLFormElement>, id: string | null) {
    event.preventDefault();
    const key = id ?? 'topic';
    const draft = drafts[key] ?? emptyComment;
    const body = draft.body.trim();
    if (!body || Array.from(body).length > 500) return;
    const action = id ? {action: 'reply', threadId: id} : {action: 'topic', ...(anchor ? {anchor} : {})};
    const input = {...action, body, mentions: draft.mentions.map(item => item.id)};
    const payload = JSON.stringify(input);
    const idempotencyKey = draft.submitted?.payload === payload ? draft.submitted.id : newOperationId();
    saveDraft(key, {...draft, submitted: {payload, id: idempotencyKey}});
    if (await command.send(input, idempotencyKey)) {
      saveDraft(key, emptyComment);
      if (id) setReplyTo(null);
      else {
        onClearAnchor();
        selectStatus('active');
      }
    }
  }

  return <div className="page-discussion">
    <div className="sharing-actions" aria-label="Фильтр обсуждения">
      <button className="space-action space-secondary" type="button" aria-pressed={!threadId && status === 'active'}
        onClick={() => selectStatus('active')}>Активные</button>
      <button className="space-action space-secondary" type="button" aria-pressed={!threadId && status === 'resolved'}
        onClick={() => selectStatus('resolved')}>Решённые</button>
    </div>
    {command.error && <p className="form-error" role="alert">
      {command.error}{' '}<button type="button" className="space-action space-secondary"
        onClick={() => setChange(value => value + 1)}>Обновить обсуждение</button>
    </p>}
    {!result && active && <p className="collaboration-hint" role="status">Загружаем обсуждение…</p>}
    {result?.error && <p className="form-error" role="alert">
      {result.error}{' '}<button type="button" className="space-action space-secondary"
        onClick={() => setChange(value => value + 1)}>Повторить</button>
    </p>}
    {result?.data && <>
      {result.data.threads.length === 0 && <p className="collaboration-hint">
        {threadId ? 'Обсуждение недоступно.' : 'Здесь пока нет веток.'}
      </p>}
      {result.data.threads.map(thread => <article key={thread.id} id={`comment-${thread.id}`}
        tabIndex={-1} className="comment-thread" aria-label={`Ветка · ${thread.author}`}>
        <AddressedComment pageTo={pageTo} thread={thread.id} comment={thread} />
        {thread.anchor && <AnchorQuote pageTo={pageTo} threadId={thread.id} anchor={thread.anchor} editing={editing} onJump={onJump} />}
        {thread.status === 'resolved' && <p className="collaboration-hint">Решено
          {thread.changedBy ? ` · ${thread.changedBy}` : ''}{' '}
          {thread.changedAt != null && <CommentTime stamp={thread.changedAt} />}
        </p>}
        {thread.replies.map(reply => <div key={reply.id} id={`comment-${reply.id}`}
          tabIndex={-1} className="comment-reply">
          <AddressedComment pageTo={pageTo} thread={thread.id} comment={reply} />
        </div>)}
        {(thread.repliesOffset ?? 0) > 0 && <button type="button" className="space-action space-secondary"
          onClick={() => { setReplyFocus(''); setRepliesOffset(0); }}>Сначала</button>}
        {thread.repliesNextOffset != null && <button type="button" className="space-action space-secondary"
          onClick={() => {
            setThreadId(thread.id);
            setReplyFocus('');
            setRepliesOffset(thread.repliesNextOffset!);
          }}>Ещё ответы</button>}
        <div className="sharing-actions">
          {thread.status === 'active' && result.data!.canComment && <button type="button"
            className="space-action space-secondary" aria-expanded={replyTo === thread.id}
            onClick={() => setReplyTo(value => value === thread.id ? null : thread.id)}>Ответить</button>}
          {thread.canChange && <button type="button" className="space-action space-secondary" disabled={command.busy}
            onClick={() => void command.send({action: thread.status === 'active' ? 'resolve' : 'reopen',
              threadId: thread.id, version: thread.version})}>
            {thread.status === 'active' ? 'Решить' : 'Открыть снова'}
          </button>}
        </div>
        {thread.status === 'active' && result.data!.canComment && replyTo === thread.id && <CommentComposer reply
          discussionTo={path} draft={drafts[thread.id] ?? emptyComment} busy={command.busy}
          onChange={draft => saveDraft(thread.id, draft)} onSend={event => void sendComment(event, thread.id)} />}
      </article>)}
      {!threadId && (offset > 0 || result.data.nextOffset != null) && <div className="collaboration-pager">
        <button className="space-action space-secondary" type="button" disabled={offset === 0}
          onClick={() => setOffsets(value => value.slice(0, -1))}>Назад</button>
        <button className="space-action space-secondary" type="button" disabled={result.data.nextOffset == null}
          onClick={() => setOffsets(value => [...value, result.data!.nextOffset!])}>Дальше</button>
      </div>}
      {result.data.canComment && (status === 'active' || anchor) && !threadId && !replyTo && <CommentComposer
        discussionTo={path} anchor={anchor} onClearAnchor={onClearAnchor} busy={command.busy}
        draft={drafts.topic ?? emptyComment} onChange={draft => saveDraft('topic', draft)}
        onSend={event => void sendComment(event, null)} />}
    </>}
  </div>;
}
