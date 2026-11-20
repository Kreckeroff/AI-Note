import type {KeyboardEvent} from 'react';

export function CollaborationTabs<Tab extends string>({id, label, tabs, value, onChange}: {
  id: string;
  label: string;
  tabs: readonly (readonly [Tab, string])[];
  value: Tab;
  onChange(tab: Tab): void;
}) {
  function move(event: KeyboardEvent<HTMLDivElement>) {
    const at = tabs.findIndex(([tab]) => tab === value);
    const last = tabs.length - 1;
    const index = event.key === 'Home' ? 0 : event.key === 'End' ? last
      : event.key === 'ArrowLeft' ? (at + last) % tabs.length
      : event.key === 'ArrowRight' ? (at + 1) % tabs.length : -1;
    if (index < 0) return;
    event.preventDefault();
    const tab = tabs[index][0];
    onChange(tab);
    document.getElementById(`${id}-tab-${tab}`)?.focus();
  }

  return <div className="collaboration-tabs" role="tablist" aria-label={label} onKeyDown={move}>
    {tabs.map(([tab, title]) => <button key={tab} type="button" className="space-action space-secondary"
      role="tab" id={`${id}-tab-${tab}`} aria-selected={value === tab}
      aria-controls={`${id}-panel-${tab}`} tabIndex={value === tab ? 0 : -1} onClick={() => onChange(tab)}>
      {title}
    </button>)}
  </div>;
}
