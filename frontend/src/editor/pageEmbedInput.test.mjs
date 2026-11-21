import assert from 'node:assert/strict';
import test from 'node:test';
import {readEmbedInput} from './pageEmbedInput.ts';

test('raw HTTPS input is passed to address validation without opening an HTML parser', () => {
  const owner = {createElement: () => assert.fail('a URL does not need HTML parsing')};
  assert.deepEqual(readEmbedInput(' https://video.example.test/embed?a=1&b=2#t=3 ', owner), {
    address: 'https://video.example.test/embed?a=1&b=2#t=3',
  });
});

test('iframe input accepts standard embed attributes and lets an inert DOM decode the sole src', () => {
  const value = `<IFRAME title='example src="ignored"' src="https://video.example.test/v?a=1&amp;b=2" allowfullscreen\nstyle="width:100%;border:0"></IFRAME>`;
  const owner = {createElement(name) {
    assert.equal(name, 'template');
    return {
      set innerHTML(markup) { assert.equal(markup, value); },
      content: {firstElementChild: {localName: 'iframe', getAttribute(key) {
        assert.equal(key, 'src');
        return 'https://video.example.test/v?a=1&b=2';
      }}},
    };
  }};
  assert.deepEqual(readEmbedInput(value, owner), {address: 'https://video.example.test/v?a=1&b=2'});
});

test('ambiguous or extra markup fails before being passed to the HTML parser', () => {
  const owner = {createElement: () => assert.fail('malformed input reaches no DOM parser')};
  for (const value of [
    '<iframe SRC="https://a.example.test" src="https://b.example.test"></iframe>',
    '<iframe src="https://a.example.test" SRCDOC=""></iframe>',
    '<iframe src="https://a.example.test">не пустой</iframe>',
    '<iframe src="https://a.example.test"></iframe><b>лишнее</b>',
    '<iframe src="https://a.example.test">',
    '<iframe title="src=https://a.example.test"></iframe>',
  ]) assert.ok('error' in readEmbedInput(value, owner), value);
});
