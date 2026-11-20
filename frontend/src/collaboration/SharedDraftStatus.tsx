import type {SharedDraftView} from './sharedDraft';

export function sharedDraftStatus(state: SharedDraftView | null): string {
  if (!state) return '';
  switch (state.status) {
    case 'opening': return 'Открываем общий черновик…';
    case 'ready': return state.pending ? 'Синхронизация…' : 'Черновик синхронизирован';
    case 'sending': return 'Синхронизация…';
    case 'offline': return state.pending ? `Нет связи · Ожидают правки (${state.pending})` : 'Нет связи';
    case 'conflict': return 'Нужно совместить правки';
    case 'expired': return 'Допуск к редактированию изменился';
    case 'recover': return 'Проверьте черновик перед продолжением';
    case 'rejected': return state.message;
  }
}

export function SharedDraftRecovery({state, onReopen, onUseCurrent, onDismissCopy, onMergeInput, onResolve}: {
  state: SharedDraftView;
  onReopen(): void;
  onUseCurrent(): void;
  onDismissCopy(): void;
  onMergeInput(text:string): void;
  onResolve(text: string): void;
}) {
  if (state.status === 'expired' || state.status === 'rejected') {
    return <section className="shared-draft-recovery">
      <p>{state.status === 'expired' ? 'Проверьте текущий доступ и сравните неподтверждённые правки.' : state.message}</p>
      <button className="space-action space-secondary" type="button" onClick={onReopen}>Обновить сеанс</button>
    </section>;
  }
  if (state.status === 'recover' && state.snapshot) {
    return <section className="shared-draft-recovery" aria-label="Восстановление черновика">
      <h2>Сохранённый текст сеанса</h2>
      <p>Откройте актуальный черновик и перенесите нужные фрагменты. Сохранённый текст останется под редактором.</p>
      <pre tabIndex={0}>{state.recovery || 'Пока здесь ничего нет.'}</pre>
      <div className="shared-draft-actions">
        <button className="space-action space-secondary" type="button" onClick={onUseCurrent}>Открыть текущий черновик</button>
      </div>
    </section>;
  }
  const conflict = state.conflict;
  const copy = state.copy !== undefined && <details className="shared-draft-recovery">
    <summary>Сохранённый текст сеанса</summary>
    <pre tabIndex={0}>{state.copy}</pre>
    <button className="space-action space-secondary" type="button" onClick={onDismissCopy}>Закрыть копию</button>
  </details>;
  if (state.status === 'conflict' && conflict) return <>
    <MergeFragment initial={conflict.mine} current={conflict.theirs} canResolve={state.canResolve}
      text={state.mergeInput ?? conflict.mine} onInput={onMergeInput}
      priorLabel={conflict.persistent ? 'Черновик до восстановления' : 'Ваш фрагмент'} onResolve={onResolve} />
    {copy}
  </>;
  return copy;
}

function MergeFragment({initial,current,canResolve,text,priorLabel,onInput,onResolve}: {
  initial:string; current:string; canResolve:boolean; text:string; priorLabel:string;
  onInput(text:string):void; onResolve(text:string):void;
}) {
  return <section className="shared-draft-recovery" aria-labelledby="shared-draft-merge">
    <h2 id="shared-draft-merge">Совместить правки</h2>
    <div className="shared-draft-versions">
      <div><h3>{priorLabel}</h3><pre tabIndex={0}>{initial}</pre></div>
      <div><h3>Текущий черновик</h3><pre tabIndex={0}>{current}</pre></div>
    </div>
    <label htmlFor="merged-draft-fragment">Итоговый фрагмент</label>
    <textarea id="merged-draft-fragment" value={text} onChange={event => onInput(event.currentTarget.value)} rows={3} />
    <button className="space-action primary-action" type="button" disabled={!canResolve} onClick={() => onResolve(text)}>Применить совмещение</button>
  </section>;
}
