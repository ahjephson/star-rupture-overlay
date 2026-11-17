import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { packageSite, publicBase } from './package.mjs';
import { pages } from './guides.mjs';
import { withSEO, previewImage } from './seo.mjs';

test('search metadata is idempotent and provides factual app data and guide breadcrumbs', () => {
  const source = '<html><head><title>Timer &amp; Compass</title><meta name="description" content="Track recipes &amp; timing."></head><body><h1>Settings and controls</h1></body></html>';
  const home = withSEO(source, 'index.html');
  assert.equal(withSEO(home, 'index.html'), home);
  assert.match(home, /property="og:title" content="Timer &amp; Compass"/);
  assert.ok(home.includes(`property="og:image" content="${previewImage}"`));
  const graph = html => JSON.parse(html.match(/<script type="application\/ld\+json">(.*?)<\/script>/s)[1])['@graph'];
  const app = graph(home).find(item => item['@type'] === 'SoftwareApplication');
  assert.equal(app.name, 'StarRupture Overlay');
  assert.equal(app.operatingSystem, 'Windows 10 or Windows 11 (64-bit)');
  assert.equal(app.aggregateRating, undefined);
  assert.equal(app.review, undefined);
  const guide = withSEO(source, 'guides/settings.html');
  const crumbs = graph(guide).find(item => item['@type'] === 'BreadcrumbList').itemListElement;
  assert.deepEqual(crumbs.map(item => item.name), ['StarRupture Overlay', 'Settings and controls']);
  assert.equal(crumbs[1].item, publicBase + 'guides/settings.html');
});

test('display screenshots stay within the image budget and full-size originals remain available', async () => {
  for (const name of ['in-game-overview', 'personal-marker-arrival']) {
    const image = await readFile(new URL(`website/assets/${name}.webp`, import.meta.url));
    assert.equal(image.toString('ascii', 8, 12), 'WEBP');
    assert.ok(image.length < 600_000, `${name}: display image exceeds 600 KB`);
    const fullSize = await readFile(new URL(`website/assets/${name}.png`, import.meta.url));
    assert.ok(fullSize.length > image.length);
  }
});

test('website publishing requires an explicit manual publish selection', async () => {
  const workflow = (await readFile(new URL('../.github/workflows/website.yml', import.meta.url), 'utf8')).replaceAll('\r\n', '\n');
  assert.match(workflow, /on:\n  workflow_dispatch:/);
  assert.doesNotMatch(workflow, /^  (push|pull_request|release|schedule):/m);
  assert.match(workflow, /publish:[\s\S]*?type: boolean\n        default: false/);
  assert.match(workflow, /if: inputs\.publish == true/);
  assert.match(workflow, /path: dist\/site\n/);
});

test('publish package excludes unused/private images and cleans stale output', async () => {
  const root = await mkdtemp(join(tmpdir(), 'overlay-site-'));
  try {
    await mkdir(join(root, 'site/website/assets'), { recursive: true });
    await mkdir(join(root, 'site/website/guides'));
    const html = '<html><head><meta name="robots" content="noindex,nofollow"></head><body><img src="assets/used.png"></body></html>';
    await writeFile(join(root, 'site/website/index.html'), html);
    for (const page of pages) await writeFile(join(root, `site/website/guides/${page.slug}.html`), html.replace('src="assets/', 'src="../assets/'));
    await writeFile(join(root, 'site/website/assets/used.png'), 'approved pixels');
    await writeFile(join(root, 'site/website/assets/in-game-overview.webp'), 'public sharing image');
    await writeFile(join(root, 'site/website/assets/private-original.png'), 'private');
    await mkdir(join(root, 'dist/site'), { recursive: true });
    await writeFile(join(root, 'dist/site/stale.png'), 'stale');
    const manifest = await packageSite(root, true);
    assert.deepEqual(manifest.filter(file => file.path.endsWith('.png')).map(file => file.path), ['assets/used.png']);
    await assert.rejects(readFile(join(root, 'dist/site/stale.png')));
    const output = await readFile(join(root, 'dist/site/index.html'), 'utf8');
    assert.match(output, /content="index,follow"/);
    assert.ok(output.includes(`rel="canonical" href="${publicBase}"`));
    assert.equal(await readFile(join(root, 'site/website/index.html'), 'utf8'), html, 'Preview source stays noindex');
    await packageSite(root);
    assert.match(await readFile(join(root, 'dist/site/index.html'), 'utf8'), /noindex,nofollow/);
    await writeFile(join(root, 'site/website/index.html'), html.replace('assets/used.png', '../private.png'));
    await assert.rejects(packageSite(root), /Unapproved public file/);
  } finally { await rm(root, { recursive: true, force: true }); }
});
