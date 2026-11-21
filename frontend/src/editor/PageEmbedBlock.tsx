import type {ExternalEmbedAddress} from './markdownModel';

type EditControls = {onEdit(opener: HTMLElement): void; onRemove(): void};

export function PageEmbedBlock({embed, controls, blockStart}: {blockStart?: number; embed: ExternalEmbedAddress; controls?: EditControls}) {
  const teamStream = embed.kind === 'teamstream';
  return <figure data-editor-block={blockStart} className="page-embed" data-page-embed="" data-page-embed-kind={embed.kind} contentEditable={false}>
    <figcaption className="page-embed-heading">
      <strong className="page-embed-host">{teamStream ? 'TeamStream' : embed.host}</strong>
      {controls && <details className="page-embed-editor-menu" onKeyDown={event => {
        if (event.key !== 'Escape' || !event.currentTarget.open) return;
        event.preventDefault();
        event.currentTarget.open = false;
        event.currentTarget.querySelector('summary')?.focus();
      }}>
        <summary className="space-action space-secondary" aria-label="Действия с блоком">···</summary>
        <div className="page-embed-editor-actions">
          <button type="button" className="space-action space-secondary" onClick={event => {
            const menu = event.currentTarget.closest('details');
            const opener = menu?.querySelector('summary');
            if (!menu || !opener) return;
            menu.open = false;
            controls.onEdit(opener);
          }}>Изменить адрес</button>
          <button type="button" className="space-action space-secondary" onClick={controls.onRemove}>Удалить блок</button>
        </div>
      </details>}
    </figcaption>
    <div className="page-embed-placeholder" data-page-embed-placeholder="">
      <button type="button" className="space-action space-secondary" data-page-embed-load="">
        {teamStream ? 'Открыть плеер' : 'Загрузить'}
      </button>
    </div>
    <div className="page-embed-frame" data-page-embed-frame="" hidden />
    <div className="page-embed-actions">
      <a data-page-embed-source="" href={embed.url} target="_blank" rel="noopener noreferrer">
        {teamStream ? 'Открыть в TeamStream' : 'Открыть источник'}
      </a>
      <button type="button" className="space-action space-secondary" data-page-embed-hide="" hidden>Скрыть</button>
      <span data-page-embed-help="" hidden>{teamStream
        ? 'Если видео не открывается, проверьте доступ в TeamStream.'
        : 'Если блок не открывается, откройте источник.'}</span>
    </div>
  </figure>;
}
