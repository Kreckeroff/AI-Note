export const maxMarkdownSourceLength = 60_000;
const sizeError = 'Файл должен содержать не больше 60 000 символов.';

export async function readMarkdownFile(file: File): Promise<string> {
  if (!/\.(md|markdown)$/i.test(file.name)) throw new Error('Выберите файл с расширением .md или .markdown.');
  // Four UTF-8 bytes per Unicode scalar plus the optional UTF-8 BOM.
  if (file.size > maxMarkdownSourceLength * 4 + 3) throw new Error(sizeError);
  let bytes: ArrayBuffer;
  try {
    bytes = await file.arrayBuffer();
  } catch {
    throw new Error('Не удалось прочитать файл. Выберите его снова.');
  }
  let source: string;
  try {
    source = new TextDecoder('utf-8', {fatal:true}).decode(bytes).replace(/\r\n?/g, '\n');
  } catch {
    throw new Error('Сохраните файл в кодировке UTF-8 и выберите его снова.');
  }
  if (/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u.test(source)) {
    throw new Error('Выберите текстовый Markdown-файл.');
  }
  if (Array.from(source).length > maxMarkdownSourceLength) throw new Error(sizeError);
  return source;
}
