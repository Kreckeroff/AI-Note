import {useId, useLayoutEffect, useRef, useState, type KeyboardEvent} from 'react';
import {Icon} from '../Icon';

export type ListStyle = 'bullet' | 'ordered' | 'task';

const styles = [
  {command: 'bullet', label: 'Маркированный список', icon: 'list'},
  {command: 'ordered', label: 'Нумерованный список', icon: 'ordered'},
  {command: 'task', label: 'Чекбокс', icon: 'check'},
] as const;

type Props = {
  disabled: boolean;
  active: boolean;
  onFocus(): void;
  onApply(style: ListStyle): void;
};

export function ListStyleMenu({disabled, active, onFocus, onApply}: Props) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const id = useId();
  const arrowOpened = useRef(false);

  useLayoutEffect(() => {
    if (!open) return;
    if (arrowOpened.current) {
      root.current?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus();
      arrowOpened.current = false;
    }
    function dismiss(event: PointerEvent) {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false);
    }
    function position() {
      const panel = menu.current;
      const button = trigger.current?.getBoundingClientRect();
      if (!panel || !button) return;
      const left = Math.max(8, Math.min(button.right - panel.offsetWidth, window.innerWidth - panel.offsetWidth - 8));
      const top = button.bottom + panel.offsetHeight + 6 > window.innerHeight ? button.top - panel.offsetHeight - 6 : button.bottom + 4;
      panel.style.left = `${left}px`;
      panel.style.top = `${Math.max(8, top)}px`;
    }
    position();
    document.addEventListener('pointerdown', dismiss);
    window.addEventListener('resize', position);
    window.addEventListener('scroll', position, true);
    return () => {
      document.removeEventListener('pointerdown', dismiss);
      window.removeEventListener('resize', position);
      window.removeEventListener('scroll', position, true);
    };
  }, [open]);

  function onMenuKey(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      setOpen(false);
      trigger.current?.focus();
      return;
    }
    if (event.key === 'Tab') {
      setOpen(false);
      trigger.current?.focus();
      return;
    }
    const buttons = Array.from(event.currentTarget.querySelectorAll('button'));
    const current = buttons.indexOf(event.target as HTMLButtonElement);
    const next = event.key === 'ArrowDown' ? current + 1 : event.key === 'ArrowUp' ? current - 1 :
      event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : undefined;
    if (next === undefined) return;
    event.preventDefault();
    event.stopPropagation();
    buttons[(next + buttons.length) % buttons.length]?.focus();
  }

  return <div ref={root} className="page-list-tools" onBlur={event => {
    if (event.relatedTarget instanceof Node && !event.currentTarget.contains(event.relatedTarget)) setOpen(false);
  }}>
    <button ref={trigger} data-markdown-tool="" type="button" disabled={disabled} tabIndex={active ? 0 : -1}
      className="page-list-disclosure" aria-label="Варианты списка" aria-haspopup="menu" aria-expanded={open}
      aria-controls={open ? id : undefined} onFocus={onFocus}
      onClick={event => {
        arrowOpened.current = event.detail === 0;
        setOpen(value => !value);
      }}
      onKeyDown={event => {
        if (event.key === 'Escape' && open) {
          event.preventDefault();
          setOpen(false);
        } else if (event.key === 'ArrowDown') {
          event.preventDefault();
          event.stopPropagation();
          if (open) root.current?.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus();
          else {
            arrowOpened.current = true;
            setOpen(true);
          }
        }
      }}><Icon name="down" /></button>
    {open && !disabled && <div ref={menu} className="page-list-menu" id={id} role="menu" aria-label="Варианты списка" onKeyDown={onMenuKey}>
      {styles.map(style => <button key={style.command} type="button" role="menuitem" tabIndex={-1}
        onClick={() => { setOpen(false); onApply(style.command); }}>
        <Icon name={style.icon} />{style.label}
      </button>)}
    </div>}
  </div>;
}
