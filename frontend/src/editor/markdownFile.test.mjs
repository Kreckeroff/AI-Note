import assert from 'node:assert/strict';
import test from 'node:test';
import {readMarkdownFile} from './markdownFile.ts';

test('imports Unicode Markdown without rewriting unsupported blocks, whitespace or final newline', async () => {
  const source = '# 🌲\r\n\r\n~~~text\rraw <b>content</b>  \r\n~~~\r\n';
  assert.equal(await readMarkdownFile(new File(['\ufeff', source], 'Конспект.MD')), source.replace(/\r\n?/g, '\n'));
});

test('rejects an incorrect extension, malformed UTF-8 and binary content without substituting characters', async () => {
  await assert.rejects(readMarkdownFile(new File(['text'], 'document.md.exe')), /\.md/);
  await assert.rejects(readMarkdownFile(new File([new Uint8Array([0xc3, 0x28])], 'broken.md')), /UTF-8/);
  await assert.rejects(readMarkdownFile(new File(['ab\u0000cd'], 'binary.md')), /текстовый/);
});

test('counts Unicode code points after line ending normalization and bounds reads before decoding', async () => {
  assert.equal((await readMarkdownFile(new File(['🧭'.repeat(60000)], 'limit.md'))).length, 120000);
  await assert.rejects(readMarkdownFile(new File(['🧭'.repeat(60001)], 'big.md')), /60 000/);
  await assert.rejects(readMarkdownFile({name:'big.md',size:5000000, arrayBuffer:() => assert.fail()}), /60 000/);
  assert.equal(await readMarkdownFile(new File([], 'empty.md')), '');
});
