(() => {
  const teamStreamOrigin = 'https://workspace.teamstream.mws.ru';
  const teamStreamPath = /^\/embed\/[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}$/;
  const normalizeHost = hostname => hostname.replace(/\.+$/, '');

  function externalAddress(link) {
    if (!(link instanceof HTMLAnchorElement) || !link.hasAttribute('href')) return null;
    let url;
    try { url = new URL(link.href); } catch { return null; }
    if (url.protocol !== 'https:' || url.username || url.password ||
        normalizeHost(url.hostname) === normalizeHost(location.hostname)) return null;
    return url;
  }

  function teamStreamWrapper() {
    const origin = document.querySelector('head meta[name="sever-teamstream-origin"]')?.content;
    if (!origin) return null;
    let wrapper;
    try { wrapper = new URL(origin); } catch { return null; }
    const loopback = /^(?:localhost|\[::1\]|127(?:\.[0-9]{1,3}){3})$/.test(wrapper.hostname);
    if (wrapper.origin !== origin || normalizeHost(wrapper.hostname) !== wrapper.hostname ||
        wrapper.hostname === normalizeHost(location.hostname) || wrapper.hostname === 'workspace.teamstream.mws.ru' ||
        !(wrapper.protocol === 'https:' || wrapper.protocol === 'http:' && loopback)) return null;
    return wrapper;
  }

  function teamStreamFrameAddress(url) {
    if (url.origin !== teamStreamOrigin || !teamStreamPath.test(url.pathname)) return null;
    const suffix = url.href.slice(url.origin.length + url.pathname.length);
    const seconds = /^\?t=([0-9]+)$/.exec(suffix)?.[1];
    if (suffix !== '' && (seconds === undefined || Number(seconds) > 0xffffffff)) return null;
    const wrapper = teamStreamWrapper();
    if (!wrapper) return null;
    wrapper.pathname = url.pathname.toLowerCase();
    if (seconds !== undefined) wrapper.searchParams.set('t', String(Number(seconds)));
    return wrapper;
  }

  function loadFrame(region, source, teamStream) {
    const url = teamStream ? teamStreamFrameAddress(source) : source;
    if (!url) return false;
    const frame = document.createElement('iframe');
    frame.title = teamStream ? 'Видео TeamStream' : `Внешнее содержимое · ${url.hostname}`;
    frame.referrerPolicy = 'no-referrer';
    frame.setAttribute('sandbox', teamStream ? 'allow-scripts allow-same-origin' : 'allow-scripts allow-forms');
    const fullscreen = teamStream ? `fullscreen 'src' ${teamStreamOrigin}` : 'fullscreen';
    frame.allow = `${fullscreen}; autoplay 'none'; camera 'none'; microphone 'none'; geolocation 'none'`;
    frame.src = url.href;
    region.replaceChildren(frame);
    return true;
  }

  document.addEventListener('click', event => {
    if (!(event.target instanceof Element)) return;
    const button = event.target.closest('[data-page-embed-load], [data-page-embed-hide]');
    if (!(button instanceof HTMLButtonElement) || button.hidden || button.disabled) return;
    const block = button.closest('[data-page-embed]');
    if (!block) return;

    const region = block.querySelector('[data-page-embed-frame]');
    const placeholder = block.querySelector('[data-page-embed-placeholder]');
    const load = block.querySelector('[data-page-embed-load]');
    const hide = block.querySelector('[data-page-embed-hide]');
    const help = block.querySelector('[data-page-embed-help]');
    if (!region || !placeholder || !load || !hide || !help) return;

    const opening = button === load;
    if (opening) {
      const url = externalAddress(block.querySelector('[data-page-embed-source]'));
      if (!url || region.firstElementChild) return;
      if (!loadFrame(region, url, block.dataset.pageEmbedKind === 'teamstream')) return;
    } else {
      region.replaceChildren();
    }
    placeholder.hidden = opening;
    region.hidden = hide.hidden = help.hidden = !opening;
    (opening ? hide : load).focus({preventScroll: true});
  });
})();
