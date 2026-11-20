import {readPage} from '../model';

export async function readPublishedPage(path: string, signal: AbortSignal) {
  const response = await fetch(path, {credentials:'same-origin',cache:'no-store',redirect:'error',signal});
  if (!response.ok) return;
  const html = new DOMParser().parseFromString(await response.text(),'text/html');
  const page = readPage(html.getElementById('sever-ui-data')?.textContent ?? '');
  if (page?.kind !== 'closedSpace' || !page.tree.reading || !page.tree.selectedPage) return;
  const expected = `/workspace/${page.workspace.tenant.id}/spaces/${page.space.id}/pages/${page.tree.selectedPage.id}`;
  if (expected === path) return {reading:page.tree.reading,page:page.tree.selectedPage};
}
