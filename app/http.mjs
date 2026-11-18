const maxFormBytes = 4_096;
const securityHeaders = {
  'Cache-Control': 'no-store',
  'Content-Security-Policy': "default-src 'none'; script-src 'self'; style-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
  'X-Content-Type-Options': 'nosniff',
  'Referrer-Policy': 'same-origin',
};

export function send(response, status, body, headers = {}) {
  if (response.destroyed) return;
  response.writeHead(status, {...securityHeaders, 'Content-Type': 'text/html; charset=utf-8', ...headers});
  response.end(body);
}

export function redirect(response, location, headers = {}) {
  send(response, 303, '', {Location: location, ...headers});
}

export function plain(response, status, message, headers = {}) {
  send(response, status, message, {'Content-Type': 'text/plain; charset=utf-8', ...headers});
}

export function hasFormEncoding(request) {
  return request.headers['content-type']?.split(';', 1)[0].trim().toLowerCase() === 'application/x-www-form-urlencoded';
}

export async function readForm(request) {
  if (Number(request.headers['content-length']) > maxFormBytes) return undefined;
  const chunks = [];
  let length = 0;
  for await (const chunk of request.iterator({destroyOnReturn: false})) {
    length += chunk.length;
    if (length > maxFormBytes) return undefined;
    chunks.push(chunk);
  }
  return new URLSearchParams(Buffer.concat(chunks).toString('utf8'));
}

export function formValue(form, name) {
  const values = form.getAll(name);
  return values.length === 1 ? values[0] : '';
}
