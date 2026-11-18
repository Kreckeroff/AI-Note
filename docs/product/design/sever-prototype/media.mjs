export const rasterAccept = 'image/png,image/jpeg,image/webp';

export async function prepareCover(file) {
  if (!rasterAccept.split(',').includes(file.type)) throw new Error('Выберите растровое изображение PNG, JPEG или WebP.');
  if (file.size > 5*1024*1024) throw new Error('Размер исходного изображения должен быть до 5 МБ.');
  const bitmap = await createImageBitmap(file);
  try {
    const target = document.createElement('canvas');
    target.width = 1000;
    target.height = 340;
    const context = target.getContext('2d',{alpha:false});
    const scale = Math.max(target.width/bitmap.width,target.height/bitmap.height);
    const width = bitmap.width*scale, height = bitmap.height*scale;
    context.drawImage(bitmap,(target.width-width)/2,(target.height-height)/2,width,height);
    const image = target.toDataURL('image/webp',.77);
    if (!image.startsWith('data:image/webp;base64,') || image.length > 390000) throw new Error('Изображение не удалось подготовить. Выберите файл меньшего размера.');
    return image;
  } finally { bitmap.close(); }
}

export function coverImage(images = {},key) {
  const source = images[key];
  return typeof source === 'string' && /^data:image\/webp;base64,[\w+/=]+$/.test(source) ? source : '';
}

export function parseVideo(input) {
  try {
    const url = new URL(input);
    if (url.protocol !== 'https:' || url.username || url.password) return undefined;
    const host = url.hostname.toLowerCase();
    const ytb = ['www.youtube.com','youtube.com','m.youtube.com'].includes(host) && url.pathname === '/watch' ? url.searchParams.get('v') :
      host === 'youtu.be' ? url.pathname.slice(1) : '';
    if (/^[\w-]{11}$/.test(ytb || '')) return {source:'https://www.youtube.com/watch?v='+ytb, embed:'https://www.youtube-nocookie.com/embed/'+ytb,label:'Видео YouTube'};
    const vimeo = /^(?:www\.)?vimeo\.com$/.test(host) && /^\/\d{1,14}$/.test(url.pathname) ? url.pathname.slice(1) : '';
    if (vimeo) return {source:'https://vimeo.com/'+vimeo,embed:'https://player.vimeo.com/video/'+vimeo,label:'Видео Vimeo'};
    if (/\.(mp4|webm)$/i.test(url.pathname)) return {source:url.href, direct:true,label:'Видео'};
  } catch { /* Invalid or unsupported video address. */ }
  return undefined;
}
