/** Check the preview's generated pages and local navigation without external requests. */
import assert from 'node:assert/strict';
import { readFile, access } from 'node:fs/promises';
import { pages } from './guides.mjs';
const root = new URL('./website/', import.meta.url);
let links = 0;
for (const path of ['index.html', ...pages.map(page => `guides/${page.slug}.html`)]) {
  const url = new URL(path, root);
  const html = await readFile(url, 'utf8');
  assert.equal((html.match(/<h1\b/g) || []).length, 1, `${path}: one primary heading`);
  assert.match(html, /name="robots" content="noindex,nofollow"/, `${path}: preview stays noindex`);
  assert.doesNotMatch(html, /\u2014/, `${path}: no em dashes`);
  assert.doesNotMatch(html, /pre-wave-pending|compass-side-arrow-pending/, `${path}: held-back captures must stay unused`);
  assert.doesNotMatch(html, /website-screenshot-originals|codex-clipboard-|crop-manifest/, `${path}: private source captures must not be linked`);
  assert.equal((html.match(/rel="canonical"/g) || []).length, 1, `${path}: one canonical URL`);
  assert.match(html, /property="og:url" content="https:\/\/ahjephson\.dev\/star-rupture-overlay\//);
  assert.match(html, /name="twitter:card" content="summary_large_image"/);
  assert.doesNotMatch(html, /\b(?:PTB|PTR|public test (?:branch|realm|region))\b/i, path + ": use stable Update 3 wording");
  const schema = JSON.parse(html.match(/<script type="application\/ld\+json">(.*?)<\/script>/s)[1]);
  assert.equal(schema['@context'], 'https://schema.org');
  assert.ok(schema['@graph'].some(item => item['@type'] === (path === 'index.html' ? 'SoftwareApplication' : 'BreadcrumbList')));
  for (const [image] of html.matchAll(/<img\b[^>]*>/g)) {
    assert.match(image, /\bwidth="\d+"/, `${path}: reserve image width`);
    assert.match(image, /\bheight="\d+"/, `${path}: reserve image height`);
  }
  const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
  assert.equal(ids.length, new Set(ids).size, `${path}: IDs must be unique`);
  for (const [, value] of html.matchAll(/(?:href|src|poster)="([^"]+)"/g)) {
    if (/^https?:/.test(value)) continue;
    const target = new URL(value, url);
    if (target.pathname.endsWith('/')) target.pathname += 'index.html';
    await access(target);
    if (target.hash) {
      const content = await readFile(target, 'utf8');
      assert.ok(content.includes(`id="${target.hash.slice(1)}"`), `${path}: missing anchor ${value}`);
    }
    links++;
  }
}
const home = await readFile(new URL('index.html', root), 'utf8');
assert.match(home, /Update 2:<\/strong> one pinned recipe, controlled by keyboard shortcut/);
assert.match(home, /Update 3:<\/strong> up to three pins/);
const publicSite = 'https://ahjephson.dev/star-rupture-overlay/';
const latestNotes = publicSite + 'releases/latest';
assert.ok(home.includes('href="' + publicSite + 'downloads/starrupture-overlay-v1.0.0-windows-x64.zip">Download for Update 2'));
assert.ok(home.includes('href="https://github.com/ahjephson/star-rupture-overlay/releases/tag/v1.0.0">See what’s changed in 1.0.0'));
assert.ok(home.includes('href="' + publicSite + 'downloads/latest.zip">Download for Update 3'));
assert.ok(home.includes('href="' + latestNotes + '">See what’s changed ↗'));
const install = await readFile(new URL('guides/install.html', root), 'utf8');
assert.ok(install.includes('href="' + latestNotes + '">latest Update 3 release'));
assert.doesNotMatch(home, /latest\.json|latest-release-links/);
assert.doesNotMatch(home, /v1\.1\.0-update3\.1|UPDATE 3 PREVIEW|UPDATE 3 PRE-RELEASE/);
const codexTabs = [...home.matchAll(/<input type="radio" name="codex-tab" id="codex-tab-([a-z]+)"( checked)?>/g)];
assert.deepEqual(codexTabs.map(match => match[1]), ['production', 'buildings', 'interiors', 'equipment', 'corporations']);
assert.deepEqual(codexTabs.filter(match => match[2]).map(match => match[1]), ['production']);
for (const [, tab] of codexTabs) {
  assert.ok(home.includes(`<label for="codex-tab-${tab}">`));
  assert.ok(home.includes(`id="codex-slide-${tab}"`));
  assert.ok(home.includes(`src="assets/codex/${tab}.webp"`));
}
assert.match(home, /restores your recipe pins after restarts/);
assert.match(home, /Compass close-up with a Personal Marker flag and a distance of 41 metres/);
assert.match(home, /Separate close-up of the cyan ARRIVED cue/);
assert.match(home, /CURRENT \/ FACTORY BACKGROUND/);
assert.match(home, /ALTERNATIVE \/ CLEAR BACKGROUND/);
assert.match(home, /src="assets\/compass-clear.png"/);
assert.equal((home.match(/class="timer-stage"/g) || []).length, 6, 'Show all six captured timer stages');
for (const stage of ['STABLE','PRE-WAVE','APPROACHING','INCINERATING','COOLING','GROWBACK']) {
  assert.ok(home.includes(`alt="${stage} timer stage,`), `Missing actual timer capture: ${stage}`);
}
console.log(`Checked 5 pages and ${links} local links/assets: headings, IDs, anchors, noindex, version distinctions and Codex carousel passed.`);
