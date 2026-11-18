const paths = {
  north: 'M12 2l2.8 7.2L22 12l-7.2 2.8L12 22l-2.8-7.2L2 12l7.2-2.8L12 2z M12 9v6m-3-3h6',
  add: 'M12 5v14 M5 12h14',
  folder: 'M3 7V5a1 1 0 011-1h5l2 2h9a1 1 0 011 1v12H3z',
  lock: 'M5 10h14v11H5z m3 0V7a4 4 0 118 0v3',
  layers: 'm3 8 9-5 9 5-9 5-9-5zm0 5 9 5 9-5m-18 5 9 5 9-5',
  back: 'M20 12H5m6 6-6-6 6-6',
  close: 'm6 6 12 12M6 18 18 6',
  home: 'm3 10 9-7 9 7v10H3zm6 10v-6h6v6',
  down: 'm6 9 6 6 6-6',
  next: 'm9 6 6 6-6 6',
  search: 'M20 20l-4-4 M18 11a7 7 0 11-14 0 7 7 0 0114 0z',
};

export function Icon({name = 'north'}: {name?: keyof typeof paths}) {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor"
      strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d={paths[name]} />
    </svg>
  );
}
