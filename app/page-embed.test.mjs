import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {runInNewContext} from 'node:vm';

const script = readFileSync(new URL('./page-embed.js', import.meta.url), 'utf8');
const id = 'a1b2c3d4-0000-4000-8000-000000000001';
const provider = 'https://workspace.teamstream.mws.ru';
const external = provider + '/embed/' + id;
const wrapper = 'http://127.0.0.1:4124';

function fixture(url, kind, origin = wrapper) {
  class Element {
    hidden = false;
    disabled = false;
    focus() { focused = this; }
  }
  class Anchor extends Element {
    href = url;
    getAttribute(name) { return name === 'href' ? url : null; }
    hasAttribute(name) { return this.getAttribute(name) !== null; }
  }
  class Button extends Element {
    closest(selector) { return selector === '[data-page-embed]' ? block : this; }
  }
  let listener;
  let focused;
  const load = new Button();
  const hide = new Button();
  const source = new Anchor();
  const placeholder = new Element();
  const help = new Element();
  const region = {replaceChildren(frame) { this.firstElementChild = frame; }};
  const block = {
    dataset:{pageEmbedKind:kind},
    querySelector(selector) {
      return {
        '[data-page-embed-frame]':region, '[data-page-embed-placeholder]':placeholder,
        '[data-page-embed-load]':load, '[data-page-embed-hide]':hide, '[data-page-embed-help]':help,
        '[data-page-embed-source]':source,
      }[selector];
    },
  };
  runInNewContext(script, {
    URL, Element, HTMLAnchorElement:Anchor, HTMLButtonElement:Button,
    location:{hostname:'localhost'},
    document:{
      querySelector: selector => origin && selector === 'head meta[name="sever-teamstream-origin"]' ? {content:origin} : null,
      addEventListener: (name, action) => { assert.equal(name, 'click'); listener = action; },
      createElement: name => {
        assert.equal(name, 'iframe');
        return {setAttribute(key, value) { this[key] = value; }};
      },
    },
  });
  return {
    get frame() { return region.firstElementChild; },
    get focused() { return focused; },
    load, hide, placeholder,
    click: target => listener({target}),
  };
}

test('TeamStream opens the configured wrapper only on click and removes it on hide', () => {
  const block = fixture(provider + '/embed/' + id.toUpperCase() + '?t=00030', 'teamstream');
  assert.equal(block.frame, undefined);
  block.click(block.load);
  assert.equal(block.frame.src, wrapper + '/embed/' + id + '?t=30');
  assert.equal(block.frame.title, 'Видео TeamStream');
  assert.equal(block.frame.sandbox, 'allow-scripts allow-same-origin');
  assert.equal(block.frame.referrerPolicy,'no-referrer');
  assert.match(block.frame.allow, /fullscreen 'src' https:\/\/workspace\.teamstream\.mws\.ru/);
  assert.match(block.frame.allow, /autoplay 'none'/);
  assert.equal(block.placeholder.hidden, true);
  assert.equal(block.focused, block.hide);
  block.click(block.hide);
  assert.equal(block.frame, undefined);
  assert.equal(block.focused, block.load);
});

test('invalid source or configuration leaves the external link as the only path', () => {
  for (const [url, origin] of [
    [external, null], [external, 'http://localhost:4124'], [external, 'http://dev.example.test'],
    [external, 'https://isolated.example.test/path'], [external, provider],
    [external.replace('mws.ru', 'mws.ru.example.test'), wrapper],
    [external + '?t=1&t=2', wrapper], [external + '#', wrapper], [external + '?', wrapper],
    [provider + '/EMBED/' + id, wrapper],
  ]) {
    const block = fixture(url, 'teamstream', origin);
    block.click(block.load);
    assert.equal(block.frame, undefined, String(origin));
  }
});

test('ordinary iframe retains opaque sandbox and the original source', () => {
  const url = 'https://www.example.test/document';
  const block = fixture(url, undefined);
  block.click(block.load);
  assert.equal(block.frame.src, url);
  assert.equal(block.frame.sandbox, 'allow-scripts allow-forms');
});
