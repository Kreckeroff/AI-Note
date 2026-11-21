import {escapeHtml as html, icon} from './ui.mjs';
import {withWorkspaceLock} from './collaboration.mjs?v=tenant-switch-2';
import {migrateLegacyDemoCatalogue, tenantDemoScope} from './model.mjs?v=tenant-catalog-2';

export function usesTenantDemo(params) {
  return params.get('demo') === 'tenants' || params.get('journey') === 'markdown';
}

export async function prepareTenantDemo() {
  try { await withWorkspaceLock(migrateLegacyDemoCatalogue,tenantDemoScope); }
  catch { /* Browser storage restrictions cannot prevent the demo from opening. */ }
}

export function tenantDemoOptions(current, theme, {organization, personal}) {
  return [['admin',organization,'Организация','north'],['personal',personal,'Личный тенант','user']]
    .map(([scenario,name,description,glyph]) => {
      const content = `<span class="entity-icon">${icon(glyph,16)}</span><span class="entity-text"><strong>${html(name)}</strong><span>${description}</span></span>`;
      if (scenario === current) {
        return `<div class="entity tenant-option" aria-current="true">${content}<span class="tag">Текущая среда</span></div>`;
      }
      const query = new URLSearchParams({demo:'tenants',scenario,theme});
      return `<a class="entity tenant-option" href="kb-spc-02.html?${query}#spaces">${content}${icon('chevron',14)}</a>`;
    }).join('');
}
