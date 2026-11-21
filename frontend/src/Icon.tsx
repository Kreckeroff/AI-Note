import {workspaceIcons} from './workspaceIcons';

export function Icon({name = 'north'}: {name?: keyof typeof workspaceIcons}) {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round">
      <path d={workspaceIcons[name]} />
    </svg>
  );
}
