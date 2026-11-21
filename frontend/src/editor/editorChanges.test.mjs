import assert from 'node:assert/strict';
import {registerHooks} from 'node:module';
import test from 'node:test';

const imports = registerHooks({
  resolve(specifier, context, next) {
    return next(specifier === './markdownModel' ? './markdownModel.ts' : specifier, context);
  },
});
const {parseMarkdown} = await import('./markdownModel.ts');
const {
  applyCommand, applySourceCommand, insertMenuBlock, pasteMarkdown, applyLink,
  insertVisual, removeVisual, moveListItem, toggleTask, updateCodeLanguage, exitCodeBlock,
  insertExternalEmbed, updateExternalEmbed,
} = await import('./editorChanges.ts');
imports.deregister();

const place = (source, word) => {
  const start = source.indexOf(word);
  return {start, end: start + word.length};
};

const caret = (position) => ({start:position,end:position});

test('block picker inserts the selected heading level below the active paragraph and preserves adjacent Markdown', () => {
  const source = '**Один** абзац\n\nТретий';
  for (const [command, prefix] of [['heading-one','#'],['heading','##'],['heading-three','###']]) {
    const result = insertMenuBlock(source, false, caret(3), command);
    assert.equal(result.source, '**Один** абзац\n\n' + prefix + ' Заголовок\n\nТретий');
    assert.equal(result.source.slice(0,result.start).endsWith('Заголовок'), true);
    assert.equal(result.start, result.end);
    assert.equal(insertMenuBlock('/h', false, {start:0,end:2}, command).source, prefix+' Заголовок');
  }
});

test('opening blocks from the picker uses the empty line or the end of a nested list', () => {
  const list = '- Один\n  - Внутри\n- Последний';
  assert.equal(insertMenuBlock(list, false, caret(3), 'quote').source, list+'\n\n> Цитата');
  assert.equal(insertMenuBlock('', true, caret(0), 'task').source, '- [ ] Пункт списка');
  assert.equal(insertMenuBlock('сохрани', false, {start:1,end:3}, 'quote').source, 'сохрани');
  const embed = insertExternalEmbed('Первый\n\nВторой', false, caret(2), 'https://video.example.test/watch');
  assert.ok(embed.source.startsWith('Первый\n\n```sever-embed\nhttps://video.example.test/watch\n```\n\nВторой'));
});

test('an embed consumes the slash command, preserves its paragraph and keeps a caret after the block', () => {
  const address = 'https://video.example.test/player?a=1&b=2#watch';
  const source = 'До **пример** /встроить и после';
  const inserted = insertExternalEmbed(source, false, place(source, '/встроить'), address);
  const fence = `\`\`\`sever-embed\n${address}\n\`\`\``;
  assert.equal(inserted.source, `До **пример**  и после\n\n${fence}\n\n`);
  assert.equal(inserted.start, inserted.source.length - 1);
  assert.equal(inserted.end, inserted.start);
  assert.equal(insertExternalEmbed('/встроить', true, {start:0,end:9}, address).source, fence + '\n\n');
});

test('changing or removing an embed touches only the matching atomic block', () => {
  const address = 'https://video.example.test/embed';
  const next = 'https://teamstream.example.test/view?id=8#section';
  const fence = `~~~sever-embed\n${address}\n~~~`;
  const source = `${fence}\n\nРаздел\n\n${fence}`;
  const second = {start: source.lastIndexOf(fence), end:source.length};
  const updated = updateExternalEmbed(source, false, second, address, next);
  assert.equal(updated.change.source, source.slice(0, second.start) + fence.replace(address, next));
  assert.equal(source.slice(updated.before.start, updated.before.end), address);
  assert.equal(updateExternalEmbed(updated.change.source, false, second, address, next), undefined);
  const removed = updateExternalEmbed(source, false, second, address);
  assert.equal(removed.change.source, source.slice(0, second.start));
  assert.deepEqual(removed.before, second);
  assert.equal(insertVisual(fence, false, caret(fence.length), 'После', 'typed').source, fence + '\n\nПосле');
  assert.equal(insertVisual(fence, false, caret(fence.indexOf('https') + 1), 'x').source, fence);
});

test('inserting a local image preserves adjacent text and escapes its alternate description', async () => {
  const {insertAttachmentImage} = await import('./editorChanges.ts');
  const id = '7c2c2b1e-5348-4ddb-89ec-7e4bfeeddcad';
  const original = 'До /изо и после';
  const result = insertAttachmentImage(original,false,place(original,'/изо'),id,'Схема [A | B]');
  const inserted = parseMarkdown(result.source,false).runs.find(run => run.attachment?.id === id);
  assert.equal(inserted?.attachment.alt,'Схема [A | B]');
  assert.equal(result.source.slice(result.end),' и после');
  assert.ok(result.source.startsWith('До !['));
  assert.equal(insertAttachmentImage('`код`',false,caret(3),id,'x').source,'`код`');
  assert.equal(insertAttachmentImage('AB',false,caret(1),id,'').source,`A![](attachment:${id})B`);
});

test('removing one attachment strips only its recognized references, preserving code, escaped examples and other files', async () => {
  const {attachmentRanges, removeAttachmentReferences} = await import('./editorChanges.ts');
  const id = '7c2c2b1e-5348-4ddb-89ec-7e4bfeeddcad';
  const other = 'ab637c85-504e-4d68-b95a-f93fb42bf08d';
  const image = `![Схема](attachment:${id})`;
  const link = `[Файл](attachment:${id})`;
  const innocent = [`\\!\\${image.slice(1)}`, '`' + image + '`', '```', image, '```', `![Другая](attachment:${other})`].join('\n');
  const source = `До ${image} после\n\n${link}\n${innocent}`;
  assert.equal(attachmentRanges(source,false,id).length,2);
  assert.equal(removeAttachmentReferences(source,false,caret(source.length),id).source,`До  после\n\n\n${innocent}`);
  assert.equal(removeAttachmentReferences(source,true,caret(0),id).source,source);
});

test('attachment removal uses raw canonical tokens and permits an empty label only for images', async () => {
  const {attachmentRanges, removeAttachmentReferences} = await import('./editorChanges.ts');
  const id = '7c2c2b1e-5348-4ddb-89ec-7e4bfeeddcad';
  const literal = [
    `![Пример](attachment\\:${id})`,
    `[Пример](attachment:${id.replace('-', '\\-')})`,
    `[](attachment:${id})`,
    `![Регистр](attachment:${id.toUpperCase()})`,
    '`' + `![](attachment:${id})` + '`',
  ].join('\n');
  const reference = `![](attachment:${id})`;
  const ordinaryLink = '[Сайт](https\\://example.test/a\\(b\\))';
  const source = `${literal}\n\n${reference} и ${ordinaryLink}`;
  assert.deepEqual(attachmentRanges(source,false,id).map(({start,end})=>source.slice(start,end)),[reference]);
  assert.equal(removeAttachmentReferences(source,false,caret(source.length),id).source,`${literal}\n\n и ${ordinaryLink}`);
  const references = parseMarkdown(source,false).runs.flatMap(run => run.attachment ? [run.attachment] : []);
  assert.deepEqual(references,[{id,alt:'',image:true}]);
  assert.ok(parseMarkdown(source,false).runs.some(run => run.marks.some(mark => mark.href === 'https://example.test/a(b)')));
});

test('striking a selection preserves other inline marks and can be reversed locally', () => {
  const source = 'До **слово** после';
  const marked = applyCommand(source,false,place(source,'слово'),'strike');
  const match = parseMarkdown(marked.source,false).runs.find(run=>run.text==='слово');
  assert.deepEqual(match.marks.map(mark=>mark.kind).sort(), ['bold','strike']);
  assert.equal(applyCommand(marked.source,false,place(marked.source,'слово'),'strike').source, source);
  const part = applyCommand(marked.source,false,place(marked.source,'лов'),'strike');
  assert.deepEqual(styledCharacters(part.source).slice(3,8), [
    ['с','bold+strike'], ['л','bold'], ['о','bold'], ['в','bold'], ['о','bold+strike'],
  ]);
});

test('ordered commands create one list and continue visible numbering and marker spelling', () => {
  const source = 'До\n\nПервый\n\nВторой\n\nПосле';
  assert.equal(applyCommand(source,false,{start:4,end:source.indexOf('Второй')+6},'ordered').source,
    'До\n\n1. Первый\n2. Второй\n\nПосле');
  const custom = '7) первый\n1) второй\nне список';
  const next = insertVisual(custom,false,caret(custom.indexOf('второй')+6),'\n','paragraph');
  assert.equal(next.source,'7) первый\n1) второй\n9) \nне список');
});

test('Enter inserts a sibling after the parent subtree and keeps the split inline marks', () => {
  const source = '7) **Родитель**\n   - [x] Деталь\n8) Следующий';
  const text = place(source,'Родитель');
  const after = insertVisual(source,false,caret(text.end),'\n','paragraph');
  assert.equal(after.source,'7) **Родитель**\n   - [x] Деталь\n8) \n8) Следующий');
  assert.equal(after.start,after.source.indexOf('8) ') + 3);

  const inside = insertVisual(source,false,caret(text.start + 3),'\n','paragraph');
  assert.equal(inside.source,'7) **Род**\n   - [x] Деталь\n8) **итель**\n8) Следующий');
  assert.equal(inside.start,inside.source.indexOf('итель'));

  const empty = '7) \n   - [x] Деталь\n8) Следующий';
  const continued = insertVisual(empty,false,caret(3),'\n','paragraph');
  assert.equal(continued.source,'7) \n   - [x] Деталь\n8) \n8) Следующий');
  assert.equal(continued.start,continued.source.indexOf('8) ') + 3);
});

test('Enter exits only at the end of a list, preserving following nested siblings', () => {
  const source = '- Родитель\n  - [ ] \n  - [x] Сосед\n- После';
  const offset = source.indexOf('\n  - [x]');
  const next = insertVisual(source,false,caret(offset),'\n','paragraph');
  assert.equal(next.source,'- Родитель\n  - [ ] \n  - [ ] \n  - [x] Сосед\n- После');
  assert.equal(next.start,offset + '\n  - [ ] '.length);
  assert.equal(parseMarkdown(next.source,false).blocks[3].task.checked,true);

  const ordered = '7) \n8) Следующий';
  assert.equal(insertVisual(ordered,false,caret(3),'\n','paragraph').source,'7) \n8) \n8) Следующий');
  const last = '7) Первый\n8) ';
  assert.equal(insertVisual(last,false,caret(last.length),'\n','paragraph').source,'7) Первый\n\n');

  assert.equal(insertVisual('7) \n- Следующий',false,caret(3),'\n','paragraph').source,'\n\n- Следующий');
  assert.equal(insertVisual('- [ ] \n7) Следующий',false,caret(6),'\n','paragraph').source,'\n\n7) Следующий');
});

test('explicit bullet style converts ordered items and keeps existing bullets and descendants', () => {
  const ordered = '7) Родитель\n   - [x] Деталь\n8) Сосед';
  const range = place(ordered,'Родитель');
  assert.equal(applyCommand(ordered,false,range,'bullet').source,'- Родитель\n   - [x] Деталь\n8) Сосед');
  assert.equal(applySourceCommand(ordered,range.start,range.end,'bullet').source,'- Родитель\n   - [x] Деталь\n8) Сосед');
  const bullet = '- Родитель\n   - [x] Деталь\n- Сосед';
  const parent = place(bullet,'Родитель');
  assert.equal(applyCommand(bullet,false,parent,'bullet').source,bullet);
  assert.equal(applySourceCommand(bullet,parent.start,parent.end,'bullet').source,bullet);
  assert.equal(applyCommand(bullet,false,parent,'list').source,bullet);
  assert.equal(applyCommand('- Пункт',false,caret(4),'list').source,'Пункт');
});

test('source list styles apply to selected lines or the current line with stable offsets', () => {
  const source = 'До  \nПервый  \n\t\nВторой\nПосле';
  const start = source.indexOf('рвый');
  const end = source.indexOf('После');
  const ordered = applySourceCommand(source,start,end,'ordered');
  assert.deepEqual(ordered,{
    source:'До  \n1. Первый  \n2. Второй\nПосле',
    start:start + 3,
    end:end + 4,
  });
  assert.equal(applySourceCommand(source,start,start,'task').source,'До  \n- [ ] Первый  \n\t\nВторой\nПосле');
  assert.equal(applySourceCommand('Первый\nВторой',0,14,'ordered').source,'1. Первый\n2. Второй');
  const separated = 'До\n\nПервый\n\nВторой\n\nПосле';
  assert.equal(applySourceCommand(separated,4,separated.indexOf('Второй') + 6,'ordered').source,
    'До\n\n1. Первый\n2. Второй\n\nПосле');
});

test('list movement keeps a subtree and outdent leaves the former siblings with their parent', () => {
  const source = '- Родитель\n  4) Первый\n     - Внук\n  - Второй\n- После';
  const range = place(source, 'Первый');
  const raised = moveListItem(source,false,range,'outdent');
  assert.equal(raised.source,'- Родитель\n  - Второй\n4) Первый\n   - Внук\n- После');
  assert.equal(raised.source.slice(raised.start,raised.end),'Первый');
  const lowered = moveListItem(raised.source,false,raised,'indent');
  assert.equal(lowered.source,'- Родитель\n  - Второй\n  4) Первый\n     - Внук\n- После');
  assert.equal(moveListItem(source,false,place(source,'Родитель'),'indent').source,source);
});

test('a task changes only its state marker, and continuation adds an unchecked sibling', () => {
  const source = '- [X] Готово  \n- После';
  const checked = place(source,'Готово');
  assert.equal(toggleTask(source,false,checked).source,'- [ ] Готово  \n- После');
  assert.equal(applyCommand(source,false,checked,'task').source,source);
  const next = insertVisual(source,false,caret(source.indexOf('\n')),'\n','paragraph');
  assert.equal(next.source,'- [X] Готово  \n- [ ] \n- После');
});

test('slash commands replace the request and keep the rest of a nonempty block together', () => {
  const source = 'До \\/цит после  \n\nСосед';
  const edited = insertMenuBlock(source,false,place(source,'\\/цит'),'quote');
  assert.equal(edited.source,'До  после  \n\n> Цитата\n\nСосед');
  const exit = insertVisual(edited.source,false,edited,'\n','paragraph');
  assert.ok(!parseMarkdown(exit.source,false).blocks.find(block=>block.start<=exit.start && exit.start<=block.end).kind.includes('quote'));
  const rule = insertMenuBlock('\\/разд',true,place('\\/разд','\\/разд'),'rule');
  assert.equal(parseMarkdown(rule.source,false).blocks[0].kind,'rule');
  assert.equal(rule.source,'---\n\n');
});

test('slash insertion and Markdown paste preserve list ancestry and following siblings', () => {
  const source = '- Родитель \\/цит\n  - Ребёнок\n- Сосед\n\nПосле';
  const query = place(source,'\\/цит');
  const list = '- Родитель \n  - Ребёнок\n- Сосед';
  const quoted = insertMenuBlock(source,false,query,'quote');
  assert.equal(quoted.source,list + '\n\n> Цитата\n\nПосле');
  assert.equal(quoted.start,quoted.source.indexOf('Цитата') + 'Цитата'.length);
  const child = parseMarkdown(quoted.source,false).blocks[1];
  assert.deepEqual([child.kind,child.indent],['bullet',2]);
  assert.equal(pasteMarkdown(source,false,query,'# Тема').source,list + '\n\n# Тема\n\nПосле');
  assert.equal(pasteMarkdown(source,false,caret(query.end),'# Тема').source,
    source.replace('\n\nПосле','\n\n# Тема\n\nПосле'));

  const quote = '> Первая \\/разд\n> Вторая\n\nПосле';
  assert.equal(insertMenuBlock(quote,false,place(quote,'\\/разд'),'rule').source,
    '> Первая \n> Вторая\n\n---\n\nПосле');
});

test('multiline code preserves the body, extends a colliding fence and edits its language', () => {
  const source = 'До\n\n~~~   text  \n    код  \n~~~\n\nПосле';
  const body = place(source,'код');
  const inserted = insertVisual(source,false,body,'строка\n~~~\n\tданные  ');
  assert.equal(inserted.source,'До\n\n~~~~   text  \n    строка\n~~~\n\tданные    \n~~~~\n\nПосле');
  const language = updateCodeLanguage(inserted.source,false,inserted,'custom-unknown');
  assert.equal(language.source,inserted.source.replace('text','custom-unknown'));
  const exited = exitCodeBlock(language.source,false,language);
  assert.equal(exited.source,language.source);
  assert.equal(exited.start,exited.source.indexOf('После')-1);

  const selected = 'Первая\nВторая';
  const edit = applyCommand(selected,false,{start:0,end:selected.length},'code');
  const code = parseMarkdown(edit.source,false).blocks[0];
  assert.equal(code.kind,'fence');
  assert.equal(code.runs[0].text,selected);
});

test('a potential fence closer with trailing Unicode whitespace stays literal in code', () => {
  const source = '```\nx\n```';
  const body = '```\u00a0';
  const edit = insertVisual(source,false,place(source,'x'),body);
  assert.equal(edit.source,'````\n' + body + '\n````');
  const {blocks} = parseMarkdown(edit.source,false);
  assert.equal(blocks.length,1);
  assert.equal(blocks[0].runs[0].text,body);
});

test('replacing an adjacent language beginning with tildes preserves the original fence', () => {
  const source = '~~~text\ncode\n~~~\nПосле';
  const selected = place(source,'code');
  const edit = updateCodeLanguage(source,false,selected,'~~~lang');
  assert.equal(edit.source,'~~~ ~~~lang\ncode\n~~~\nПосле');
  const {blocks} = parseMarkdown(edit.source,false);
  assert.equal(blocks[0].fence.language,'~~~lang');
  assert.equal(blocks[1].runs[0].text,'После');
  assert.equal(edit.source.slice(edit.start,edit.end),'code');
});

test('table cell editing escapes a pipe and preserves each separator', () => {
  const source = '| A | Б |\n| --- | --- |\n| Да | Нет |';
  const edited = insertVisual(source,false,place(source,'Да'),'Да | ещё');
  assert.equal(edited.source,'| A | Б |\n| --- | --- |\n| Да \\| ещё | Нет |');
  assert.equal(insertVisual(source,false,{start:source.indexOf('Да'),end:source.indexOf('Нет')+3},'замена').source,source);
  const divider = source.indexOf('Нет');
  assert.equal(removeVisual(source,false,caret(divider),'backward').source,source);
});

test('link applies a safe address to the selected formatted label', () => {
  const source = 'До **слово** после';
  const linked = applyLink(source,false,place(source,'слово'),'слово','https://example.test/гайд');
  const match = parseMarkdown(linked.source,false).runs.find(run=>run.text==='слово');
  assert.deepEqual(match.marks.map(mark=>mark.kind).sort(),['bold','link']);
  assert.equal(linked.source.slice(linked.start,linked.end).replaceAll('*',''),'слово');
  assert.ok(linked.source.startsWith('До ') && linked.source.endsWith(' после'));
  assert.equal(applyLink(source,false,place(source,'слово'),'слово','/локально').source,source);
  const partial = applyLink(source,false,place(source,'лов'),'лов','https://example.test/');
  assert.deepEqual(styledCharacters(partial.source).slice(3,8), [
    ['с','bold'], ['л','bold+link'], ['о','bold+link'], ['в','bold+link'], ['о','bold'],
  ]);
});

test('link preserves literal valid URL punctuation in a table cell', () => {
  const source = '| Подпись | Следующая |\n| --- | --- |';
  const href = 'https://example.com/a(b)?t=(x)|z';
  const edit = applyLink(source,false,place(source,'Подпись'),'Подпись',href);
  const {blocks} = parseMarkdown(edit.source,false);
  assert.equal(blocks[0].kind,'table');
  assert.equal(blocks[0].runs.find(run=>run.text==='Подпись').marks[0].href,href);
});

test('explicit markdown paste keeps raw text, adjacent source and prior literal rendering', () => {
  const source = 'До \\/вст после  \n\nСосед';
  const markdown = '# Заголовок\r\n\r\n2) Два\r\n2) Три  ';
  assert.equal(pasteMarkdown(source,false,place(source,'\\/вст'),markdown).source,
    'До  после  \n\n# Заголовок\n\n2) Два\n2) Три  \n\nСосед');
  const legacy = '~~буквально~~\n\nОтсюда';
  const pasted = pasteMarkdown(legacy,true,place(legacy,'Отсюда'),'> Цитата');
  const first = parseMarkdown(pasted.source,false).blocks[0];
  assert.equal(first.runs.map(run=>run.text).join(''),'~~буквально~~');
  assert.ok(first.runs.every(run=>run.marks.length===0));
  const fence = '~~~\n**буквально**\n~~~\n\nТочка';
  const filled = pasteMarkdown(fence,true,place(fence,'Точка'),'> Цитата');
  assert.equal(parseMarkdown(filled.source,false).blocks[1].runs.map(run=>run.text).join(''),'**буквально**');
});

test('source commands retain selection and keep unrelated Markdown bytes', () => {
  const source = '## Тема\n\nДо важно после  \n\n\n\\*текст\\*';
  const {start, end} = place(source, 'важно');
  assert.deepEqual(applySourceCommand(source, start, end, 'bold'), {
    source: source.replace('важно', '**важно**'), start: start + 2, end: end + 2,
  });
  assert.equal(applySourceCommand('a`b', 0, 3, 'code').source, '``a`b``');
});

test('visual replacement edits one formatted range and preserves source bytes', () => {
  const source = '## Заголовок\n\nДо __значимо__ и _слово_.  \n\n\n~~~text\n**код**\n~~~';
  const selection = place(source, 'значимо');
  const edit = insertVisual(source, false, selection, 'важно');
  assert.equal(edit.source, source.replace('значимо', 'важно'));
  assert.equal(removeVisual(edit.source, false, place(edit.source, 'важно'), 'backward').source, source.replace('__значимо__', ''));
  const protectedRange = {start: source.indexOf('слово'), end: source.indexOf('код') + 1};
  assert.equal(insertVisual(source, false, protectedRange, 'тест').source, source);
});

test('plain visual input edits old source locally and punctuation stays literal', () => {
  const source = 'Первая  \n\n\\*буквально\\* и важно.';
  const edit = insertVisual(source, true, place(source, 'важно'), '<b>');
  assert.equal(edit.source, source.replace('важно', '\\<b\\>'));
  assert.equal(edit.legacy, true);
});

test('visual selection across an emphasis edge preserves the surviving run', () => {
  const source = 'До **важного** после.';
  const first = insertVisual(source, false, {start: 0, end: source.indexOf('ного')}, 'Новое ');
  assert.equal(first.source, 'Новое **ного** после.');
  const result = removeVisual(source, false, {start: source.indexOf('ного'), end: source.indexOf('ле.')}, 'backward');
  assert.equal(result.source, 'До **важ**ле.');
});

test('editing inside code retains the author’s delimiters when possible', () => {
  const source = 'До ````аб```` после  ';
  const edited = insertVisual(source, false, place(source, 'б'), 'в');
  assert.equal(edited.source, source.replace('б', 'в'));
});

test('commands toggle visual marks, preserve nested marks and render both emphasis kinds', () => {
  const source = 'До *термин* после.';
  const selection = place(source, 'термин');
  const bold = applyCommand(source, false, selection, 'bold');
  assert.equal(bold.source, 'До ***термин*** после.');
  const both = parseMarkdown(bold.source, false).runs.find(run => run.text === 'термин');
  assert.deepEqual(both.marks.map(mark => mark.kind).sort(), ['bold', 'italic']);
  assert.equal(applyCommand(bold.source, false, place(bold.source, 'термин'), 'bold').source, source);
});

test('heading and list act on the current block and list Enter exits on an empty item', () => {
  assert.equal(applyCommand('До\n\nРаздел\n\nПосле', false, place('До\n\nРаздел\n\nПосле', 'зде'), 'heading').source.replace('## Раздел', ''), 'До\n\n\n\nПосле');
  const list = applyCommand('Один\nДва', true, {start: 0, end: 8}, 'list');
  assert.equal(list.source, '- Один\n- Два');
  const item = insertVisual(list.source, false, {start: list.source.length, end: list.source.length}, '\n', 'paragraph');
  assert.equal(item.source, '- Один\n- Два\n- ');
  assert.equal(insertVisual(item.source, false, {start:item.start,end:item.end}, '\n', 'paragraph').source, '- Один\n- Два\n\n');
});

test('selected paragraphs become one list without empty items at blank separators', () => {
  const source = 'Сверху\n\nПервый\n\nВторой\n\nСнизу';
  const range = {start: source.indexOf('Первый'), end: source.indexOf('Второй') + 'Второй'.length};
  const list = applyCommand(source, false, range, 'list');
  assert.equal(list.source, 'Сверху\n\n- Первый\n- Второй\n\nСнизу');
  assert.equal(list.source.slice(list.start, list.end), 'Первый\n- Второй');

  const newItem = insertVisual(list.source, false, {start:list.end,end:list.end}, '\n','paragraph');
  const third = insertVisual(newItem.source,false,newItem,'Третий');
  const empty = insertVisual(third.source,false,third,'\n','paragraph');
  const exit = insertVisual(empty.source,false,empty,'\n','paragraph');
  const finished = insertVisual(exit.source,false,exit,'Абзац после');
  assert.equal(finished.source, 'Сверху\n\n- Первый\n- Второй\n- Третий\n\nАбзац после\n\nСнизу');
});

test('list conversion keeps outer blank rows and still starts a list at an empty caret', () => {
  const source = '\n\t\nПервый  \n\n \t\nВторой\n\n\n';
  const list = applyCommand(source, false, {start:0,end:source.length}, 'list');
  assert.equal(list.source, '\n\t\n- Первый  \n- Второй\n\n\n');
  const blank = applyCommand('', true, {start:0,end:0}, 'list');
  assert.deepEqual(blank, {source:'- ',legacy:false,start:2,end:2});
  assert.equal(insertVisual(blank.source,false,blank,'\n','paragraph').source,'');
});

test('enter splits a list item and preserves the format on both sides', () => {
  const source = '- **важно**\n- Осталось  ';
  const offset = source.indexOf('жно');
  const result = insertVisual(source, false, {start: offset, end: offset}, '\n', 'paragraph');
  assert.equal(result.source, '- **ва**\n- **жно**\n- Осталось  ');
  assert.deepEqual(parseMarkdown(result.source, false).blocks.slice(0, 2).map(block =>
    block.runs.filter(run => run.text).map(run => [run.text, run.marks[0]?.kind])), [
    [['ва', 'bold']],
    [['жно', 'bold']],
  ]);
});

test('code over selected text keeps the unselected halves of other marks', () => {
  const source = 'До **важно** после';
  const leading = applyCommand(source, false, {start: 0, end: source.indexOf('жно')}, 'code');
  assert.equal(leading.source, '`До ва`**жно** после');
  const trailing = applyCommand(source, false, {start: source.indexOf('жно'), end: source.length}, 'code');
  assert.equal(trailing.source, 'До **ва**`жно после`');
  const code = 'До `abcdef` после';
  assert.deepEqual(applyCommand(code, false, place(code, 'cd'), 'code'), {source: code, legacy: false, ...place(code, 'cd')});
});

test('the code command merges adjacent code spans without displaying delimiter characters', () => {
  const left = '`ab`rest';
  assert.equal(applyCommand(left,false,place(left,'rest'),'code').source, '`abrest`');
  const right = 'rest`ab`';
  assert.equal(applyCommand(right,false,place(right,'rest'),'code').source, '`restab`');
  const both = '`ab`rest`cd`';
  const merged = applyCommand(both,false,place(both,'rest'),'code');
  assert.equal(merged.source, '`abrestcd`');
  assert.equal(parseMarkdown(merged.source,false).runs.find(run=>run.text)?.marks[0]?.kind,'code');
  assert.equal(applyCommand(left,false,{start:4,end:4},'code').source, '`abкод`rest');
});

test('deleting around a protected block cannot remove its line separators', () => {
  const source = 'До\n```text\nданные\n```\nПосле';
  const beforeFence = source.indexOf('`') - 1;
  const afterFence = source.indexOf('После');
  assert.equal(removeVisual(source, false, {start:beforeFence,end:beforeFence}, 'forward').source, source);
  assert.equal(removeVisual(source, false, {start:afterFence,end:afterFence}, 'backward').source, source);
  assert.equal(removeVisual(source, false, {start:afterFence-1,end:afterFence}, 'backward').source, source);
});

test('bold applies only to selected plain runs and skips inline code', () => {
  const source = 'До **важно** `x` после  ';
  const edit = applyCommand(source, false, {start: 0, end: source.indexOf('жно')}, 'bold');
  assert.equal(edit.source, '**До** **важно** `x` после  ');
  const full = applyCommand(source, false, {start: 0, end: source.indexOf('  ')}, 'bold');
  assert.equal(full.source, '**До** **важно** `x` **после**  ');
  const italic = '_важное_';
  const toggled = applyCommand(italic, false, place(italic, 'важ'), 'italic');
  assert.deepEqual(parseMarkdown(toggled.source, false).runs.filter(run=>run.text).map(run=>[run.text, run.marks[0]?.kind]), [
    ['важ', undefined], ['ное', 'italic'],
  ]);
});

function styledCharacters(source) {
  return parseMarkdown(source, false).runs.flatMap(run=>Array.from(run.text, character=>
    [character, run.marks.map(mark=>mark.kind).sort().join('+')]));
}

test('partial bold removal preserves combined italic, including prefix, middle and suffix', () => {
  for (const marker of ['***','___']) {
    const source = 'До ' + marker + 'важно' + marker + ' после  ';
    for (const selected of ['ва','ажн','жно']) {
      const after = applyCommand(source,false,place(source,selected),'bold');
      const visible = styledCharacters(after.source);
      const at = 'важно'.indexOf(selected);
      assert.deepEqual(visible.slice(3,8), Array.from('важно',(char,i)=>
        [char,at<=i && i<at+selected.length ? 'italic':'bold+italic']));
      assert.ok(after.source.startsWith('До ') && after.source.endsWith(' после  '));
    }
  }
});

test('partial bold removal preserves nested italic written with different delimiters', () => {
  const source = 'До **слово _термин_ слово** после.';
  const after = applyCommand(source,false,place(source,'термин'),'bold');
  const expected = styledCharacters(source);
  for (let index = 9; index < 15; index++) expected[index][1] = 'italic';
  const output = styledCharacters(after.source);
  assert.deepEqual(output.filter(row=>row[0].trim()), expected.filter(row=>row[0].trim()));
});

test('replacing the contents of a nested mark moves boundary spaces outside the stack', () => {
  const source = 'До ***термин*** после  ';
  const after = insertVisual(source,false,place(source,'термин'),' новый ');
  assert.equal(after.source,'До  ***новый***  после  ');
  assert.deepEqual(styledCharacters(after.source).slice(4,9),Array.from('новый',char=>[char,'bold+italic']));
});

test('the visual scanner keeps tag-like source literal and recognizes the shared block grammar', () => {
  const source = '  ###\tТема  \n+\tОдин\n*\tДва\n2)\tТри\n>\tЦитата\n\nДо <span title="**важно**"> после\n\nA | B\n--- | ---\n**literal** | C\n  - nested';
  const model = parseMarkdown(source, false);
  assert.deepEqual(model.blocks.filter(row=>row.bodyStart !== row.end).map(row=>row.kind), [
    'heading','bullet','bullet','ordered','quote','paragraph','table','protected',
  ]);
  assert.equal(model.blocks[0].runs[0].text, 'Тема  ');
  const tag = model.runs.find(run=>run.text.includes('<span'));
  assert.deepEqual([tag.text, tag.marks, tag.locked], ['<span title="**важно**">', [], true]);
  const table = model.blocks.find(block=>block.kind==='table');
  const cellText = table.table.rows.map(row=>row.cells.map(cell=>cell.runs.map(run=>run.text).join('').trim()));
  assert.deepEqual(cellText,[['A', 'B'], ['literal', 'C']]);
  assert.equal(table.table.rows[1].cells[0].runs.find(run=>run.text==='literal').marks[0].kind, 'bold');
  assert.equal(table.locked, false);
});

test('protected literal tag spelling does not acquire visible escapes from visual editing', () => {
  const source = 'До <sample value="abc"> после';
  assert.equal(insertVisual(source,false,place(source,'abc'),'*важно*').source,source);
  const changed = insertVisual(source,false,place(source,'после'),'ещё');
  assert.equal(changed.source,'До <sample value="abc"> ещё');
});

test('enabling Markdown preserves tag-like text on a legacy page', () => {
  const source = 'До <sample value="**abc**"> после';
  const promoted = applyCommand(source,true,place(source,'после'),'bold');
  assert.equal(promoted.source,'До <sample value="**abc**"> **после**');
  assert.ok(!parseMarkdown(promoted.source,false).runs.find(run=>run.text.includes('abc')).marks.length);
});

test('promoting a page preserves every visible character of a legacy malformed table', () => {
  const lines = ['| A | B |', '| --- | --- |', '| bad |'];
  const source = lines.join('\n') + '\n\nслово';
  const edit = applyCommand(source,true,place(source,'слово'),'strike');
  const blocks = parseMarkdown(edit.source,false).blocks;
  assert.deepEqual(blocks.slice(0,3).map(block=>block.runs.map(run=>run.text).join('')),lines);
  assert.ok(blocks.slice(0,3).every(block=>block.kind==='paragraph'));
  assert.equal(edit.source.slice(edit.start,edit.end),'слово');
  assert.ok(blocks.at(-1).runs.find(run=>run.text==='слово').marks.some(mark=>mark.kind==='strike'));
});

test('typed delimiters autoformat and code preserves literal nested syntax', () => {
  let source = 'До ';
  let caret = source.length;
  let legacy = true;
  for (const char of '`**жирный**`') {
    const edit = insertVisual(source, legacy, {start:caret,end:caret}, char, 'typed');
    source = edit.source;
    caret = edit.start;
    legacy = edit.legacy;
  }
  assert.equal(source, 'До `**жирный**`');
  const code = parseMarkdown(source, false).runs.find(run => run.text === '**жирный**');
  assert.equal(code.marks[0].kind, 'code');
  assert.deepEqual([caret, legacy], [source.length, false]);
  const followed = insertVisual(source, false,{start:caret,end:caret}, '!', 'typed');
  assert.equal(followed.source, 'До `**жирный**`\\!');
});

test('plus and slash insert below complete multiline paragraphs and list continuations', () => {
  for (const group of ['Первая строка\nвторая строка', '- Первый\nпродолжение\n- Второй']) {
    const source = group + '\n\nДальше';
    const offset = source.indexOf('р') + 1;
    const expected = group + '\n\n> Цитата\n\nДальше';
    assert.equal(insertMenuBlock(source, false, caret(offset), 'quote').source, expected);
    const token = '/цит';
    const typed = source.slice(0, offset) + token + source.slice(offset);
    assert.equal(insertMenuBlock(typed, false, place(typed, token), 'quote').source, expected);
  }
});
