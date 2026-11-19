import {Fragment, type ReactNode} from 'react';
import type {EditorBlock, MarkdownDocument, SourceRange, TextRun} from './markdownModel';
import type {PageAttachment} from './pageAttachments';
import {Icon} from '../Icon';

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

export function RichMarkdownBlocks({model, readOnly, attachments, onRemoveImage, onTask, onLanguage, onExitCode}: BlockOptions) {
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
    return first.kind === 'bullet' ? <ul key={first.start}>{items}</ul> :
      <ol key={first.start} start={first.number}>{items}</ol>;
  }

  function renderTable(block: EditorBlock) {
    const table = block.table;
    if (!table) return null;
    function renderRow(row: NonNullable<EditorBlock['table']>['rows'][number], rowIndex: number) {
      const Cell = rowIndex === 0 ? 'th' : 'td';
      return <tr key={rowIndex}>{row.cells.map((cell, column) => <Cell key={column}
        data-markdown-cell-start={cell.runs[0]?.offsets[0] ?? cell.start}
        scope={rowIndex === 0 ? 'col' : undefined}
        className={table?.alignments[column] ? `markdown-cell-${table.alignments[column]}` : undefined}>
        {contents(cell.runs)}
      </Cell>)}</tr>;
    }
    return <div key={block.start} className="markdown-table-scroll" data-markdown-table="">
      <table>
        <thead>{renderRow(table.rows[0], 0)}</thead>
        <tbody>{table.rows.slice(1).map((row, offset) => renderRow(row, offset + 1))}</tbody>
      </table>
    </div>;
  }

  function renderFence(block: EditorBlock, children: ReactNode) {
    const fence = block.fence;
    if (!fence || block.locked) return <pre key={block.start} contentEditable={false} title={protectedEdit}><code>{children}</code></pre>;
    return <div key={block.start} className="page-code-block" data-markdown-code="">
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
  while (index < model.blocks.length) {
    const block = model.blocks[index];
    if (isList(block)) {
      elements.push(renderList(block));
      continue;
    }
    index++;
    const key = block.start;
    const children = contents(block.runs);
    if (block.kind === 'fence') elements.push(renderFence(block, children));
    else if (block.kind === 'table') elements.push(renderTable(block));
    else if (block.kind === 'rule') elements.push(<hr key={key} contentEditable={false} />);
    else if (block.kind === 'protected') elements.push(<pre key={key} contentEditable={false} title={protectedEdit}>{children}</pre>);
    else if (block.kind === 'quote') elements.push(<blockquote key={key}>{children}</blockquote>);
    else if (block.kind === 'heading') {
      const Tag = block.level === 1 ? 'h2' : block.level === 2 ? 'h3' : 'h4';
      elements.push(<Tag key={key}>{children}</Tag>);
    } else elements.push(<p key={key}>{children}</p>);
  }
  return elements;
}
