import {useState} from 'react';
import {Icon} from './Icon';
import type {DocumentPage, Section} from './model';

type TreeProps = {
  sections: Section[];
  pages: DocumentPage[];
  selectedSection?: Section | null;
  selectedPage?: DocumentPage | null;
  sectionHref(id: string): string;
  pageHref(id: string): string;
  onRenamePage(page: DocumentPage, opener: HTMLButtonElement): void;
};

function group<T extends {parentId?: string | null}>(items: T[]): Map<string | null, T[]> {
  const groups = new Map<string | null, T[]>();
  for (const item of items) {
    const parent = item.parentId ?? null;
    const group = groups.get(parent);
    if (group) group.push(item);
    else groups.set(parent, [item]);
  }
  return groups;
}

export function ancestors<T extends {id: string; parentId?: string | null}>(items: T[], selected?: T | null): T[] {
  const list: T[] = [];
  const index = new Map(items.map(item => [item.id, item]));
  const visited = new Set<string>();
  let item = selected;
  while (item?.parentId && !visited.has(item.parentId)) {
    visited.add(item.parentId);
    item = index.get(item.parentId);
    if (item) list.unshift(item);
  }
  return list;
}

export function DocumentTree({sections, pages, selectedSection, selectedPage, sectionHref, pageHref, onRenamePage}: TreeProps) {
  const [expanded, setExpanded] = useState(() => new Set([
    selectedSection?.id,
    selectedPage?.id,
    ...ancestors(sections, selectedSection).map(item => item.id),
    ...ancestors(pages, selectedPage).map(item => item.id),
  ]));
  const childSections = group(sections);
  const childPages = group(pages);
  const roots = new Map<string, DocumentPage[]>();
  for (const page of childPages.get(null) ?? []) {
    const group = roots.get(page.sectionId);
    if (group) group.push(page);
    else roots.set(page.sectionId, [page]);
  }

  function toggle(id: string) {
    setExpanded(previous => {
      const next = new Set(previous);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function branch(id: string, hasChildren: boolean, title: string) {
    return hasChildren ? <button className="document-tree-branch" type="button" aria-expanded={expanded.has(id)}
      aria-label={`${expanded.has(id) ? 'Свернуть' : 'Раскрыть'} «${title}»`} onClick={() => toggle(id)}>
      <Icon name={expanded.has(id) ? 'down' : 'next'} />
    </button> : <span className="document-tree-branch" aria-hidden="true" />;
  }

  function pageRow(page: DocumentPage) {
    const children = childPages.get(page.id) ?? [];
    const current = selectedPage?.id === page.id;
    const title = current ? selectedPage.title : page.title;
    return <li key={page.id}><div className="document-tree-row">
      {branch(page.id, children.length > 0, title)}
      <a href={pageHref(page.id)} data-page-id={page.id} aria-current={current ? 'page' : undefined}>
        <Icon name="file" /><span>{title}</span>
      </a>
      {page.canEdit && <button className="document-tree-rename" type="button" aria-haspopup="dialog"
        aria-label={`Переименовать «${title}»`} title="Переименовать" onClick={event => onRenamePage(page, event.currentTarget)}>
        <Icon name="edit" />
      </button>}
    </div>
      {expanded.has(page.id) && children.length > 0 && <ul className="section-pages">{children.map(pageRow)}</ul>}
    </li>;
  }

  function sectionRow(section: Section) {
    const children = childSections.get(section.id) ?? [];
    const documents = roots.get(section.id) ?? [];
    const current = !selectedPage && selectedSection?.id === section.id;
    return <li key={section.id}><div className="document-tree-row">
      {branch(section.id, children.length + documents.length > 0, section.title)}
      <a href={sectionHref(section.id)} data-section-id={section.id} aria-current={current ? 'page' : undefined}>
        <Icon name="folder" /><span>{section.title}</span>
      </a></div>
      {expanded.has(section.id) && children.length + documents.length > 0 &&
        <ul className="section-pages">{children.map(sectionRow)}{documents.map(pageRow)}</ul>}
    </li>;
  }

  const top = childSections.get(null) ?? [];
  return top.length ? <ul className="section-list">{top.map(sectionRow)}</ul> : <p>Пока нет разделов</p>;
}
