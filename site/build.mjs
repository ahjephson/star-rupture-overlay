/** Generate the local user guides with the product page's shared header and footer. */
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { pages } from './guides.mjs';
import { withSEO } from './seo.mjs';
const root = new URL('./website/', import.meta.url);
const index = await readFile(new URL('index.html', root), 'utf8');
await writeFile(new URL('index.html', root), withSEO(index, 'index.html'));
const header = index.match(/  <header[\s\S]*?<\/header>/)[0].replace('href="./"', 'href="../"').replace('src="assets/', 'src="../assets/').replaceAll('href="#', 'href="../#').replace('href="guides/install.html"', 'href="install.html"');
const footer = index.match(/  <footer[\s\S]*?<\/footer>/)[0];
const nav = pages.map(page => `<a href="${page.slug}.html">${page.nav}</a>`).join('');
await mkdir(new URL('guides/', root), { recursive: true });
for (const page of pages) {
  const html = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="robots" content="noindex,nofollow"><meta name="theme-color" content="#111519"><meta name="description" content="${page.description}"><title>${page.title} | StarRupture Overlay</title><link rel="icon" href="../assets/overlay-mark.png"><link rel="stylesheet" href="../styles.css"></head>
<body><a class="skip" href="#main">Skip to content</a>
${header}
<main id="main" class="wrap guide-layout"><aside class="guide-nav"><p class="eyebrow">PLAYER GUIDES</p><nav aria-label="Player guides">${nav}</nav><a class="text-link" href="../#get-started">Choose a download ↗</a></aside><article class="guide-prose"><p class="guide-breadcrumb"><a href="../">StarRupture Overlay</a> / ${page.title}</p><p class="eyebrow">${page.eyebrow}</p><h1>${page.title}</h1><p class="guide-intro">${page.description}</p>${page.body}</article></main>
${footer}</body></html>`;
  await writeFile(new URL(`guides/${page.slug}.html`, root), withSEO(html, `guides/${page.slug}.html`));
}
console.log(`Built ${pages.length} player guides. Preview remains noindex.`);
