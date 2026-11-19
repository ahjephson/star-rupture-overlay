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

test('search metadata decodes entities only once', () => {
  const source = '<head><title>Timer &amp; Compass &amp;quot;</title><meta name="description" content="&quot;Track&quot; &amp;quot;"></head>';
  const html = withSEO(source, 'index.html');
  assert.ok(html.includes('property="og:title" content="Timer &amp; Compass &amp;quot;"'));
  assert.ok(html.includes('property="og:description" content="&quot;Track&quot; &amp;quot;"'));
  const json = html.match(/<script type="application\/ld\+json">([^<]+)<\/script>/)[1];
  const [page] = JSON.parse(json)['@graph'];
  assert.equal(page.name, 'Timer & Compass &quot;');
  assert.equal(page.description, '"Track" &quot;');
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

test('recipe demo uses on-demand playback and a fast-start MP4 within the video budget', async () => {
  const home = await readFile(new URL('website/index.html', import.meta.url), 'utf8');
  const player = home.match(/<video\b[\s\S]*?<\/video>/)[0];
  assert.match(player, /\bcontrols\b/);
  assert.match(player, /\bplaysinline\b/);
  assert.match(player, /preload="none"/);
  assert.doesNotMatch(player, /autoplay/);
  assert.match(player, /poster="assets\/video\/recipe-tracking-poster.webp"/);
  assert.match(player, /src="assets\/video\/recipe-tracking-demo.mp4" type="video\/mp4"/);
  const video = await readFile(new URL('website/assets/video/recipe-tracking-demo.mp4', import.meta.url));
  assert.ok(video.length < 8_000_000, 'Keep the short demo below 8 MB');
  assert.ok(video.indexOf(Buffer.from('moov')) < video.indexOf(Buffer.from('mdat')), 'Metadata precedes media for progressive playback');
});

test('Personal Marker demo has an accessible on-demand player and a fast-start MP4', async () => {
  const home = await readFile(new URL('website/index.html', import.meta.url), 'utf8');
  const players = [...home.matchAll(/<video\b[\s\S]*?<\/video>/g)].map(match => match[0]);
  assert.equal(players.length, 2);
  const player = players.find(value => value.includes('personal-marker-demo.mp4'));
  assert.match(player, /\bcontrols\b/);
  assert.match(player, /\bplaysinline\b/);
  assert.match(player, /preload="none"/);
  assert.doesNotMatch(player, /autoplay/);
  assert.match(player, /poster="assets\/video\/personal-marker-poster.webp"/);
  assert.match(player, /aria-describedby="marker-demo-caption"/);
  const video = await readFile(new URL('website/assets/video/personal-marker-demo.mp4', import.meta.url));
  assert.ok(video.length < 10_000_000, 'Keep the marker demo below 10 MB');
  assert.ok(video.indexOf(Buffer.from('moov')) < video.indexOf(Buffer.from('mdat')));
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
    await mkdir(join(root, 'site/website/assets/video'));
    await writeFile(join(root, 'site/website/assets/video/demo.mp4'), 'approved demo');
    await writeFile(join(root, 'site/website/assets/video/poster.webp'), 'approved poster');
    await writeFile(join(root, 'site/website/assets/video/unused-take.mp4'), 'private unused take');
    await writeFile(join(root, 'site/website/index.html'), html.replace('</body>', '<video controls preload="none" poster="assets/video/poster.webp"><source src="assets/video/demo.mp4" type="video/mp4"></video></body>'));
    const withVideo = await packageSite(root, true);
    assert.deepEqual(withVideo.filter(file => file.path.startsWith('assets/video/')).map(file => file.path).sort(), ['assets/video/demo.mp4', 'assets/video/poster.webp']);
    await packageSite(root);
    assert.match(await readFile(join(root, 'dist/site/index.html'), 'utf8'), /noindex,nofollow/);
    await writeFile(join(root, 'site/website/index.html'), html.replace('assets/used.png', '../private.png'));
    await assert.rejects(packageSite(root), /Unapproved public file/);
  } finally { await rm(root, { recursive: true, force: true }); }
});
