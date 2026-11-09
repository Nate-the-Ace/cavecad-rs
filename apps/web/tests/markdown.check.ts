import { renderMarkdown } from '../src/handbook/markdown.ts';
const resolve = (kind: 'link' | 'image', t: string) => (kind === 'link' ? (t === 'map' ? '#hb:map' : null) : t === 'map.png' ? '/shots/map.png' : null);
const cases: [string, string][] = [
  ['# Map', '<h1>Map</h1>'],
  ['Hello **you** and `x`', '<p>Hello <strong>you</strong> and <code>x</code></p>'],
  ['- a\n- b', '<ul><li>a</li><li>b</li></ul>'],
  ['1. a\n2. b', '<ol><li>a</li><li>b</li></ol>'],
  ['See [the map](map).', '<p>See <a href="#hb:map" data-page="map">the map</a>.</p>'],
  ['See [gone](nowhere).', '<p>See gone.</p>'],
  ['![The map](map.png)', '<p><img src="/shots/map.png" alt="The map"></p>'],
  ['<script>x</script>', '<p>&lt;script&gt;x&lt;/script&gt;</p>'],
  ['| A | B |\n| --- | --- |\n| 1 | 2 |', '<table><thead><tr><th>A</th><th>B</th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table>'],
];
let bad = 0;
for (const [md, want] of cases) {
  const got = renderMarkdown(md, resolve);
  if (got !== want) { bad++; console.log('FAIL', JSON.stringify(md), '\n got ', got, '\n want', want); }
}
console.log(bad ? 'FAILED' : 'markdown ok');
process.exit(bad ? 1 : 0);
