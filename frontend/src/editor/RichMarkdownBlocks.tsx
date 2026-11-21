import {Fragment, type ReactNode} from 'react';
import type {EditorBlock, MarkdownDocument, SourceRange, TextRun} from './markdownModel';
import type {PageAttachment} from './pageAttachments';
import {PageEmbedBlock} from './PageEmbedBlock';
import {Icon} from '../Icon';
import type {TableOperation, TablePosition} from './tableStructure';

export const caretPlaceholder = '\u200b';
export const protectedEdit = 'Чтобы изменить этот фрагмент, откройте «Исходник».';

type BlockOptions = {
  model: MarkdownDocument;
  readOnly: boolean;
  attachments: readonly PageAttachment[];
  onRemoveImage(range: SourceRange): void;
  onTask(block: EditorBlock): void;
  onLanguage(block: EditorBlock, value: string): void;
  onExitCode(block: EditorBlock): void;
  onEditEmbed(block: EditorBlock, opener: HTMLElement): void;
  onRemoveEmbed(block: EditorBlock): void;
  activeCell?: TablePosition;
  onTableChange(at: TablePosition, operation: TableOperation): void;
};

function renderRun(run: TextRun, index: number, file: PageAttachment | undefined, removeImage: (() => void) | undefined): ReactNode {
  const image = run.attachment?.image && file?.imageTo;
  const content = image ? <span data-markdown-run={index} contentEditable={false} className="page-attachment-image-editor">
    <img className="page-inline-image" src={image} alt={run.attachment?.alt ?? ''} draggable={false} />
    {removeImage && <button className="page-attachment-erase" type="button" onClick={removeImage}
      aria-label="Убрать изображение из текста"><Icon name="close" /></button>}
  </span> : <span
    data-markdown-run={index}
    contentEditable={run.locked ? false : undefined}
    className={run.locked ? 'page-rich-protected' : undefined}
    title={run.locked ? protectedEdit : undefined}
  >{run.text || caretPlaceholder}</span>;
  const wrapped = run.marks.reduceRight<ReactNode>((child, mark) => {
    if (mark.kind === 'bold') return <strong>{child}</strong>;
    if (mark.kind === 'italic') return <em>{child}</em>;
    if (mark.kind === 'strike') return <del>{child}</del>;
    if (mark.kind === 'link') return <span className="page-rich-link">{child}</span>;
    return <code>{child}</code>;
  }, content);
  return <Fragment key={index}>{wrapped}</Fragment>;
}

function isList(block?: EditorBlock): block is EditorBlock & {kind: 'bullet' | 'ordered'} {
  return block?.kind === 'bullet' || block?.kind === 'ordered';
}

export function RichMarkdownBlocks({model, readOnly, attachments, onRemoveImage, onTask, onLanguage, onExitCode,
  onEditEmbed, onRemoveEmbed, activeCell, onTableChange}: BlockOptions) {
  const indices = new Map(model.runs.map((run, index) => [run, index]));
  const files = new Map(attachments.map(file => [file.id, file]));
  const contents = (runs: TextRun[]) => runs.map(run => renderRun(run, indices.get(run)!,
    run.attachment ? files.get(run.attachment.id) : undefined,
    !readOnly && run.attachment?.image ? () => onRemoveImage({start:run.offsets[0], end:run.offsets.at(-1)!}) : undefined));
  let index = 0;

  function renderList(first: EditorBlock & {kind: 'bullet' | 'ordered'}): ReactNode {
    const indent = first.indent ?? 0;
    const items: ReactNode[] = [];
    for (;;) {
      const item = model.blocks[index];
      if (!isList(item) || item.kind !== first.kind || (item.indent ?? 0) !== indent) break;
      index++;
      const nested: ReactNode[] = [];
      for (let child = model.blocks[index]; isList(child) && (child.indent ?? 0) > indent; child = model.blocks[index]) {
        nested.push(renderList(child));
      }
      items.push(<li key={item.start} className={item.task ? 'markdown-task' : undefined}>
        {item.task && <span contentEditable={false} className="markdown-task-control">
          <input type="checkbox" checked={item.task.checked} disabled={readOnly}
            aria-label={item.runs.map(run => run.text).join('').trim() || 'Чекбокс пункта'}
            onChange={() => onTask(item)} />
        </span>}
        {contents(item.runs)}{nested}
      </li>);
    }
    return first.kind === 'bullet' ? <ul key={first.start} data-editor-block={first.start}>{items}</ul> :
      <ol key={first.start} data-editor-block={first.start} start={first.number}>{items}</ol>;
  }

  function renderTable(block: EditorBlock) {
    const table = block.table;
    if (!table) return null;
    function renderRow(row: NonNullable<EditorBlock['table']>['rows'][number], rowIndex: number) {
      const Cell = rowIndex === 0 ? 'th' : 'td';
      return <tr key={rowIndex}>{row.cells.map((cell, column) => <Cell key={column}
        data-markdown-cell-start={cell.runs[0]?.offsets[0] ?? cell.start}
        data-current-cell={activeCell?.tableStart === block.start && activeCell.row === rowIndex && activeCell.column === column || undefined}
        scope={rowIndex === 0 ? 'col' : undefined}
        className={table?.alignments[column] ? `markdown-cell-${table.alignments[column]}` : undefined}>
        {contents(cell.runs)}
      </Cell>)}</tr>;
    }
    const at = !readOnly && activeCell?.tableStart === block.start ? activeCell : undefined;
    return <div key={block.start} data-editor-block={block.start} className="markdown-editable-table">
      <div className="markdown-table-scroll" data-markdown-table={block.start}><table>
        <thead>{renderRow(table.rows[0], 0)}</thead>
        <tbody>{table.rows.slice(1).map((row, offset) => renderRow(row, offset + 1))}</tbody>
      </table></div>
      {at && <div className="markdown-table-tools" contentEditable={false} role="group" aria-label="Структура таблицы">
        <span className="markdown-cell-address">Строка {at.row + 1}, столбец {at.column + 1}</span>
        <button type="button" onClick={() => onTableChange(at, 'add-row')} aria-label="Добавить строку ниже" title="Добавить строку ниже"><Icon name="add" />Строка</button>
        <button type="button" onClick={() => onTableChange(at, 'add-column')} aria-label="Добавить столбец справа" title="Добавить столбец справа"><Icon name="add" />Столбец</button>
        <button type="button" disabled={at.row === 0} onClick={() => onTableChange(at, 'remove-row')} aria-label="Удалить строку" title="Удалить строку"><Icon name="rule" />Строка</button>
        <button type="button" disabled={table.alignments.length === 1} onClick={() => onTableChange(at, 'remove-column')} aria-label="Удалить столбец" title="Удалить столбец"><Icon name="rule" />Столбец</button>
      </div>}
    </div>;
  }

  function renderFence(block: EditorBlock, children: ReactNode) {
    const fence = block.fence;
    if (!fence || block.locked) return <pre key={block.start} data-editor-block={block.start} contentEditable={false} title={protectedEdit}><code>{children}</code></pre>;
    return <div key={block.start} data-editor-block={block.start} className="page-code-block" data-markdown-code="">
      {!readOnly && <div className="page-code-controls" contentEditable={false}>
        <label>Язык <input type="text" value={fence.language} placeholder="Без языка" spellCheck={false}
          onChange={event => onLanguage(block, event.currentTarget.value)}
          onKeyDown={event => {
            if (event.key === 'Enter' && !event.nativeEvent.isComposing) event.preventDefault();
          }} /></label>
        <button type="button" onClick={() => onExitCode(block)}>Абзац после блока</button>
      </div>}
      <pre><code>{children}</code></pre>
    </div>;
  }

  const elements: ReactNode[] = [];
  const embedCounts = new Map<string, number>();
  for (const {embed} of model.blocks) {
    if (embed) embedCounts.set(embed.url, (embedCounts.get(embed.url) ?? 0) + 1);
  }
  while (index < model.blocks.length) {
    const block = model.blocks[index];
    if (isList(block)) {
      elements.push(renderList(block));
      continue;
    }
    index++;
    const key = block.start;
    const children = contents(block.runs);
    if (block.embed) {
      const {url} = block.embed;
      // Keep unique previews through unrelated text edits. Reset identity when a URL becomes ambiguous.
      const embedKey = embedCounts.get(url) === 1 ? `embed:${url}` : `duplicate-embed:${block.start}:${url}`;
      elements.push(<PageEmbedBlock key={embedKey} blockStart={block.start} embed={block.embed}
        controls={readOnly ? undefined : {onEdit: opener => onEditEmbed(block, opener), onRemove: () => onRemoveEmbed(block)}} />);
    } else if (block.kind === 'fence') elements.push(renderFence(block, children));
    else if (block.kind === 'table') elements.push(renderTable(block));
    else if (block.kind === 'rule') elements.push(<hr key={key} data-editor-block={key} contentEditable={false} />);
    else if (block.kind === 'protected') elements.push(<pre key={key} data-editor-block={key} contentEditable={false} title={protectedEdit}>{children}</pre>);
    else if (block.kind === 'quote') elements.push(<blockquote key={key} data-editor-block={key}>{children}</blockquote>);
    else if (block.kind === 'heading') {
      const Tag = block.level === 1 ? 'h2' : block.level === 2 ? 'h3' : 'h4';
      elements.push(<Tag key={key} data-editor-block={key}>{children}</Tag>);
    } else elements.push(<p key={key} data-editor-block={key}>{children}</p>);
  }
  return elements;
}
