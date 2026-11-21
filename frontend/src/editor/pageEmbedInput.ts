export const externalPageEmbedHint = 'Адрес источника будет доступен всем, у кого есть публичная ссылка на страницу.';
export const teamStreamPublicHint = 'Для просмотра без входа установите в TeamStream доступ «Все, у кого есть ссылка».';

const iframeError = 'Вставьте один iframe с единственным атрибутом src и без srcdoc.';

function iframeMarkup(input: string): string | undefined {
  const opener = /^<iframe(?=[\t\n\f\r >])/i.exec(input);
  if (!opener) return;
  const attribute = /[\t\n\f\r ]+([^\t\n\f\r "'<>/=\x00]+)(?:[\t\n\f\r ]*=[\t\n\f\r ]*(?:"[^"]*"|'[^']*'|[^\t\n\f\r "'`=<>]+))?/y;
  attribute.lastIndex = opener[0].length;
  let sourceCount = 0;
  let end = attribute.lastIndex;
  for (let match = attribute.exec(input); match; match = attribute.exec(input)) {
    const name = match[1].toLowerCase();
    if (name === 'srcdoc') return;
    if (name === 'src') sourceCount++;
    end = attribute.lastIndex;
  }
  return sourceCount === 1 && /^[\t\n\f\r ]*>[\t\n\f\r ]*<\/iframe[\t\n\f\r ]*>$/i.test(input.slice(end))
    ? input : undefined;
}

export function readEmbedInput(value: string, owner: Document): {address: string} | {error: string} {
  const input = value.trim();
  if (!input) return {error: 'Вставьте HTTPS-адрес или код iframe.'};
  if (!input.startsWith('<')) return {address: input};
  const markup = iframeMarkup(input);
  if (!markup) return {error: iframeError};

  // An inert template decodes HTML entities without creating a browsing context.
  const template = owner.createElement('template');
  template.innerHTML = markup;
  const frame = template.content.firstElementChild;
  const address = frame?.localName === 'iframe' && frame.getAttribute('src')?.trim();
  return address ? {address} : {error: iframeError};
}
