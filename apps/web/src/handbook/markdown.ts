// The Handbook's Markdown: the small subset its pages use (headings, paragraphs, lists, bold, code, links between
// pages, images, tables). Everything a page says is escaped, so a page can never put markup or script into the app.
// `resolve` turns a link's page id or an image's file name into its address, or null when there is none (the text
// is kept, the link dropped: the Handbook check reports such links, the reader never meets a dead one).

export type Resolve = (kind: 'link' | 'image', target: string) => string | null;

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

function inline(text: string, resolve: Resolve): string {
  let out = '';
  const re = /!\[([^\]]*)\]\(([^)\s]+)\)|\[([^\]]+)\]\(([^)\s]+)\)|\*\*([^*]+)\*\*|`([^`]+)`/g;
  let at = 0;
  for (const m of text.matchAll(re)) {
    out += esc(text.slice(at, m.index));
    if (m[2] !== undefined) {
      const src = resolve('image', m[2]);
      out += src ? `<img src="${esc(src)}" alt="${esc(m[1])}">` : esc(m[1]);
    } else if (m[4] !== undefined) {
      const href = resolve('link', m[4]);
      out += href ? `<a href="${esc(href)}" data-page="${esc(m[4])}">${esc(m[3])}</a>` : esc(m[3]);
    } else if (m[5] !== undefined) out += `<strong>${esc(m[5])}</strong>`;
    else out += `<code>${esc(m[6])}</code>`;
    at = m.index + m[0].length;
  }
  return out + esc(text.slice(at));
}

const cells = (row: string) => row.trim().replace(/^\||\|$/g, '').split('|').map((c) => c.trim());

export function renderMarkdown(md: string, resolve: Resolve): string {
  const lines = md.replace(/\r\n?/g, '\n').split('\n');
  const out: string[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) { i++; continue; }
    const h = /^(#{1,3})\s+(.*)$/.exec(line);
    if (h) { out.push(`<h${h[1].length}>${inline(h[2], resolve)}</h${h[1].length}>`); i++; continue; }
    if (/^\s*\|/.test(line) && /^\s*\|?\s*-{3}/.test(lines[i + 1] ?? '')) {
      const head = cells(line).map((c) => `<th>${inline(c, resolve)}</th>`).join('');
      const body: string[] = [];
      i += 2;
      while (i < lines.length && /^\s*\|/.test(lines[i])) {
        body.push(`<tr>${cells(lines[i]).map((c) => `<td>${inline(c, resolve)}</td>`).join('')}</tr>`);
        i++;
      }
      out.push(`<table><thead><tr>${head}</tr></thead><tbody>${body.join('')}</tbody></table>`);
      continue;
    }
    const list = /^\s*(-|\d+\.)\s+/.exec(line);
    if (list) {
      const ordered = list[1] !== '-';
      const items: string[] = [];
      while (i < lines.length && (ordered ? /^\s*\d+\.\s+/ : /^\s*-\s+/).test(lines[i])) {
        items.push(`<li>${inline(lines[i].replace(/^\s*(-|\d+\.)\s+/, ''), resolve)}</li>`);
        i++;
      }
      out.push(ordered ? `<ol>${items.join('')}</ol>` : `<ul>${items.join('')}</ul>`);
      continue;
    }
    // a paragraph always takes its first line (a stray `|` line that is not a table is just text), then runs on
    // until a blank line or the start of another block
    const para: string[] = [line.trim()];
    i++;
    while (i < lines.length && lines[i].trim() && !/^(#{1,3}\s|\s*(-|\d+\.)\s|\s*\|)/.test(lines[i])) { para.push(lines[i].trim()); i++; }
    out.push(`<p>${inline(para.join(' '), resolve)}</p>`);
  }
  return out.join('');
}
