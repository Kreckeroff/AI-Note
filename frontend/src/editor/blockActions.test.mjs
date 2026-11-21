import assert from 'node:assert/strict';
import test from 'node:test';
import {blockUnits, parseMarkdown} from './markdownModel.ts';
import {registerHooks} from 'node:module';
const hook = registerHooks({resolve(s,c,next){return next(s === './markdownModel' ? './markdownModel.ts' : s,c);}});
const {changeBlock} = await import('./blockActions.ts');
hook.deregister();

function edit(source, fragment, command, literal = false) {
  const units = blockUnits(parseMarkdown(source,literal), source, literal);
  const offset = source.indexOf(fragment);
  const index = units.findIndex(unit => unit.start <= offset && offset < unit.end);
  return changeBlock(source,literal,units,index,command);
}

test('a nested list moves and duplicates as one block; untouched whitespace survives', () => {
  const list = '- Список\n  - [x] Дочерний\n    1. Третий';
  const source = '# Заголовок\n\n' + list + '\n\n**Опора**\n  \nХвост\n';
  assert.equal(edit(source,'Дочерний','down').source, '# Заголовок\n\n**Опора**\n\n'+list+'\n  \nХвост\n');
  assert.equal(edit(source,'Третий','duplicate').source, source.replace(list, list+'\n\n'+list));
  assert.equal(edit(source,'Дочерний','remove').source,'# Заголовок\n\n**Опора**\n  \nХвост\n');
  const loose = '- Родитель\n\n  - Дочерний\n\nПосле';
  assert.equal(edit(loose,'Родитель','remove'),undefined);
  assert.equal(edit(loose,'Дочерний','up'),undefined);
});

test('fences, tables and inline media move without serializing their bodies', () => {
  const fence = ['~~~js','x();','','y();','~~~'].join('\n');
  const table = '| A \\| B | C |\n| --- | :---: |\n| 1 |  2 |';
  const image = '![файл](attachment:11111111-2222-4333-8444-555555555555)';
  const source = fence+'\n\n'+table+'\n\n'+image;
  assert.equal(edit(source,'x();','down').source, table+'\n\n'+fence+'\n\n'+image);
  assert.equal(edit(source,'1 |','duplicate').source, fence+'\n\n'+table+'\n\n'+table+'\n\n'+image);
  const embed = '~~~ sever-embed\nhttps://example.org/player\n~~~';
  const media = image+'\n\n'+embed;
  assert.equal(edit(media,'player','duplicate').source,media+'\n\n'+embed);
  assert.equal(edit(media,'attachment:','down').source,embed+'\n\n'+image);
});

test('a soft-wrapped paragraph stays whole; literal text stays literal', () => {
  const source='До\n\nАбзац\nсо второй строкой\n\n### После';
  assert.equal(edit(source,'со второй','up').source,'Абзац\nсо второй строкой\n\nДо\n\n### После');
  assert.deepEqual(edit('**До**\n# После','# После','up',true),{source:'# После\n**До**',legacy:true,start:0,end:0});
  assert.equal(edit('Одна','Одна','remove').source,'');
  assert.equal(edit('Одна','Одна','up'),undefined);
  assert.equal(edit('До\n# Между\nПосле','Между','remove').source,'До\n\nПосле');
});

test('opaque containers, unterminated fences and ambiguous neighbouring syntax stay intact', () => {
  for(const source of ['До\n\n~~~js\nне закрыто','<details>\n\nСодержимое\n\n</details>','До\n===']) {
    assert.equal(edit(source,'До','down'),undefined);
    const at=source.includes('Содержимое')?'Содержимое':source.includes('закрыто')?'не закрыто':'===';
    assert.equal(edit(source,at,'remove'),undefined);
  }
});
