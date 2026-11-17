/** Build a closed set of public pages and their referenced assets. */
import assert from 'node:assert/strict';
import { readFile, writeFile, mkdir, rm, lstat } from 'node:fs/promises';
import { resolve, dirname, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { pages } from './guides.mjs';
import { withSEO } from './seo.mjs';

export const publicBase = 'https://ahjephson.dev/star-rupture-overlay/';
export async function packageSite(repository, production = false) {
  const source = resolve(repository, 'site/website');
  const output = resolve(repository, 'dist/site');
  // A fresh package prevents old or unused captures surviving a rebuild.
  for (const directory of [resolve(repository, 'dist'), output]) {
    try { assert.ok((await lstat(directory)).isDirectory(), 'Output must be a real directory'); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  const queue = ['index.html', ...pages.map(page => `guides/${page.slug}.html`)];
  const files = new Map();
  const virtualOrigin = 'https://package.invalid';
  while (queue.length) {
    const path = queue.shift();
    if (files.has(path)) continue;
    assert.match(path, /^(?:index\.html|styles\.css|guides\/[a-z-]+\.html|assets\/(?:[a-z0-9-]+\/)*[a-z0-9-]+\.(?:png|jpg|webp|svg))$/, `Unapproved public file: ${path}`);
    const file = resolve(source, path);
    assert.ok(relative(source, file) && !relative(source, file).startsWith('..' + sep));
    // Reject symlinked parents as well as linked files.
    let current = source;
    assert.ok((await lstat(current)).isDirectory());
    for (const part of path.split('/')) {
      current = resolve(current, part);
      const info = await lstat(current);
      assert.ok(!info.isSymbolicLink() && (info.isDirectory() || (info.isFile() && info.nlink === 1)), `Linked file: ${path}`);
    }
    let data = await readFile(file);
    if (/\.(html|css)$/.test(path)) {
      let text = data.toString('utf8');
      if (path.endsWith('.html')) {
        text = withSEO(text, path);
        queue.push('assets/in-game-overview.webp');
      }
      assert.doesNotMatch(text, /codex-clipboard-|website-screenshot-originals|reference-images|crop-manifest/);
      const refs = [...text.matchAll(/(?:href|src)="([^"]+)"/g)].map(match => match[1]);
      refs.push(...[...text.matchAll(/url\(['"]?([^)'"\s]+)['"]?\)/g)].map(match => match[1]));
      for (const ref of refs) {
        const target = new URL(ref, new URL(path, virtualOrigin + '/'));
        if (target.origin !== virtualOrigin) continue;
        const local = decodeURIComponent(target.pathname).slice(1);
        queue.push(local.endsWith('/') || !local ? local + 'index.html' : local);
      }
      if (path.endsWith('.html')) {
        if (production) text = text.replace('content="noindex,nofollow"', 'content="index,follow"');

      }
      data = Buffer.from(text);
    }
    files.set(path, data);
  }
  const urls = ['index.html', ...pages.map(page => `guides/${page.slug}.html`)].map(path => publicBase + (path === 'index.html' ? '' : path));
  files.set('sitemap.xml', Buffer.from(`<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">${urls.map(url => `<url><loc>${url}</loc></url>`).join('')}</urlset>\n`));
  files.set('robots.txt', Buffer.from(production ? `User-agent: *\nAllow: /\nSitemap: ${publicBase}sitemap.xml\n` : 'User-agent: *\nDisallow: /\n'));
  files.set('.nojekyll', Buffer.from(''));
  await rm(output, { recursive: true, force: true });
  await mkdir(output, { recursive: true });
  for (const [path, data] of files) {
    await mkdir(dirname(resolve(output, path)), { recursive: true });
    await writeFile(resolve(output, path), data);
  }
  const manifest = [...files].map(([path, data]) => ({ path, bytes: data.length, sha256: createHash('sha256').update(data).digest('hex') }));
  await writeFile(resolve(repository, 'dist/site-manifest.json'), JSON.stringify({ production, publicBase, files: manifest }, null, 2) + '\n');
  console.log(`Prepared ${manifest.length} public files (${manifest.filter(file => /\.(png|jpg|webp|svg)$/.test(file.path)).length} images). No upload or deployment performed.`);
  return manifest;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await packageSite(fileURLToPath(new URL('../', import.meta.url)), process.argv.includes('--production'));
}
