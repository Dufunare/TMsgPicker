import type {TextLink} from '../types';

export interface LinkedPart {text: string; url?: string}

/** Only absolute HTTP(S) URLs may become anchors or reach the native opener. */
export function safeExternalUrl(raw: string): string | undefined {
  if (/[\u0000-\u0020\u007f]/u.test(raw)) return undefined;
  try {
    const url = new URL(raw);
    if ((url.protocol === 'http:' || url.protocol === 'https:') && url.hostname) return url.href;
  } catch { /* An invalid target remains plain text. */ }
  return undefined;
}

/** Preserve existing automatic linking of visibly written HTTP(S) addresses. */
function appendVisibleUrls(parts: LinkedPart[], text: string) {
  const re = /https?:\/\/[^\s<>"']+/gi;
  let cursor = 0;
  for (const match of text.matchAll(re)) {
    const start = match.index ?? 0;
    if (start > cursor) parts.push({text: text.slice(cursor, start)});
    const raw = match[0];
    const label = raw.replace(/[.,;!?)，。！？）]+$/u, '');
    const url = safeExternalUrl(label);
    parts.push({text: label, ...(url ? {url} : {})});
    if (label.length < raw.length) parts.push({text: raw.slice(label.length)});
    cursor = start + raw.length;
  }
  if (cursor < text.length) parts.push({text: text.slice(cursor)});
}

function isUtf16Boundary(text: string, index: number): boolean {
  if (index <= 0 || index >= text.length) return true;
  const before = text.charCodeAt(index - 1);
  const after = text.charCodeAt(index);
  return !(before >= 0xd800 && before <= 0xdbff && after >= 0xdc00 && after <= 0xdfff);
}

/** TDLib offsets and JS slice() both use UTF-16 code units. Entity links
 * override auto-detected URLs, and malformed/overlapping ranges are ignored. */
export function linkedTextParts(text: string, entities: readonly TextLink[] = []): LinkedPart[] {
  const parts: LinkedPart[] = [];
  const links = entities.flatMap(link => {
    const {offset, length} = link;
    const end = offset + length;
    if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(length) ||
        offset < 0 || length <= 0 || end > text.length ||
        !isUtf16Boundary(text, offset) || !isUtf16Boundary(text, end)) return [];
    return [{start: offset, end, url: safeExternalUrl(link.url)}];
  }).sort((a, b) => a.start - b.start || b.end - a.end);
  let cursor = 0;
  for (const link of links) {
    if (link.start < cursor) continue;
    appendVisibleUrls(parts, text.slice(cursor, link.start));
    const label = text.slice(link.start, link.end);
    parts.push({text: label, ...(link.url ? {url: link.url} : {})});
    cursor = link.end;
  }
  appendVisibleUrls(parts, text.slice(cursor));
  return parts;
}
