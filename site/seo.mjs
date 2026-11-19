/** Shared search and sharing metadata for the product page and player guides. */
export const publicBase = 'https://ahjephson.dev/star-rupture-overlay/';
export const previewImage = publicBase + 'assets/in-game-overview.webp';
const escape = value => value.replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;');
const decode = value => value.replace(/&(?:amp|quot);/g, entity => entity === '&amp;' ? '&' : '"');

export function withSEO(html, path) {
  html = html.replace(/\s*<!-- SEO:start -->[\s\S]*?<!-- SEO:end -->/g, '').replace(/<link rel="canonical"[^>]*>/g, '');
  const title = decode(html.match(/<title>([^<]+)<\/title>/)?.[1] || 'StarRupture Overlay');
  const description = decode(html.match(/name="description" content="([^"]+)"/)?.[1] || 'An unofficial Windows companion for StarRupture.');
  const url = publicBase + (path === 'index.html' ? '' : path);
  const graph = [{
    '@type': 'WebPage', '@id': url + '#page', url, name: title, description, inLanguage: 'en',
    primaryImageOfPage: { '@type': 'ImageObject', url: previewImage, width: 1600, height: 899 },
    about: { '@id': publicBase + '#application' },
  }];
  if (path === 'index.html') {
    graph.push({
      '@type': 'SoftwareApplication', '@id': publicBase + '#application',
      name: 'StarRupture Overlay', url: publicBase, description,
      applicationCategory: 'UtilitiesApplication', operatingSystem: 'Windows 10 or Windows 11 (64-bit)',
      isAccessibleForFree: true, author: { '@type': 'Person', name: 'Alex Jephson', url: 'https://ahjephson.dev/' },
      license: 'https://github.com/ahjephson/star-rupture-overlay/blob/main/LICENSE',
      installUrl: publicBase + 'guides/install.html',
      softwareHelp: { '@type': 'WebPage', url: publicBase + 'guides/help.html' },
      screenshot: previewImage,
    });
  } else {
    const label = html.match(/<h1>([^<]+)<\/h1>/)?.[1] || title;
    graph[0].breadcrumb = { '@id': url + '#breadcrumb' };
    graph.push({
      '@type': 'BreadcrumbList', '@id': url + '#breadcrumb', itemListElement: [
        { '@type': 'ListItem', position: 1, name: 'StarRupture Overlay', item: publicBase },
        { '@type': 'ListItem', position: 2, name: label, item: url },
      ],
    });
  }
  const metadata = `\n<!-- SEO:start -->
<link rel="canonical" href="${url}">
<meta property="og:type" content="website">
<meta property="og:site_name" content="StarRupture Overlay">
<meta property="og:title" content="${escape(title)}">
<meta property="og:description" content="${escape(description)}">
<meta property="og:url" content="${url}">
<meta property="og:image" content="${previewImage}">
<meta property="og:image:width" content="1600">
<meta property="og:image:height" content="899">
<meta property="og:image:alt" content="StarRupture Overlay showing the timer, Personal Marker compass and recipe tracker in game.">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="${escape(title)}">
<meta name="twitter:description" content="${escape(description)}">
<meta name="twitter:image" content="${previewImage}">
<meta name="twitter:image:alt" content="StarRupture Overlay showing the timer, Personal Marker compass and recipe tracker in game.">
<script type="application/ld+json">${JSON.stringify({ '@context': 'https://schema.org', '@graph': graph }).replaceAll('<', '\\u003c')}</script>
<!-- SEO:end -->\n`;
  return html.replace(/\s*<\/head>/, metadata + '</head>');
}
