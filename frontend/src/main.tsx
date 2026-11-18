import {flushSync} from 'react-dom';
import {createRoot} from 'react-dom/client';
import {AccountPage} from './AccountPage';
import {WorkspacePage} from './WorkspacePage';
import {readPage, type PageModel} from './model';

function Application({page}: {page: PageModel}) {
  switch (page.kind) {
    case 'signIn':
    case 'registrationStart':
    case 'registrationRequested':
    case 'registrationComplete':
    case 'noTenant':
      return <AccountPage page={page} />;
    default:
      return <WorkspacePage page={page} />;
  }
}

function hasNativeInteraction(container: HTMLElement): boolean {
  if (container.querySelector('[data-workspace-choice][open]')) return true;
  if (container.contains(document.activeElement) && !document.activeElement?.id) return true;
  return Array.from(container.querySelectorAll<HTMLInputElement>('input')).some(input =>
    input.type !== 'hidden' && input.value !== input.defaultValue);
}

function enhanceNative(container: HTMLElement) {
  const scripts: string[] = [];
  if (container.querySelector('form[action="/login"]')) scripts.push('/sign-in.mjs');
  if (container.querySelector('[data-workspace-choice], [data-catalog]')) scripts.push('/workspace-menu.mjs');
  if (container.querySelector('#space-dialog, #section-dialog')) scripts.push('/space-form.mjs');

  for (const script of scripts) {
    // The Rust server also serves these modules for builds with no React bundle.
    void import(/* @vite-ignore */ script).catch(() => {
      // The native forms remain usable if the optional enhancement fails.
    });
  }
}

const container = document.getElementById('sever-root');
const page = readPage(document.getElementById('sever-ui-data')?.textContent ?? '');
if (container && (!page || hasNativeInteraction(container))) {
  enhanceNative(container);
} else if (container && page) {
  // Keep the server form if the user or password manager already started filling it.
  const activeId = container.contains(document.activeElement) ? document.activeElement?.id : undefined;
  const root = createRoot(container);
  flushSync(() => root.render(<Application page={page} />));
  container.dataset.renderer = 'react';
  if (activeId) {
    document.getElementById(activeId)?.focus({preventScroll: true});
  } else if (page.kind === 'catalog' && window.location.hash === '#create-space') {
    document.getElementById('create-space')?.focus();
  } else if (page.kind === 'closedSpace' && window.location.hash === '#create-section') {
    document.getElementById('create-section')?.focus();
  }
}
