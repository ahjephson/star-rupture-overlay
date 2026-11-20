# Product page preview

Static design source is in `website/`. The local preview stays noindex. GitHub Pages hosts the published site, and ahjephson.dev serves it under /star-rupture-overlay/. Website publishing is separate from application releases.

The layout follows WebFile's charcoal and amber product-site style. The two full in-game captures are used on the page and retain full-size links. Feature close-ups use exact pixel crops. Both captures show Update 3. The compass has separate marker/distance and arrival views.

Run `node site/build.mjs` after editing `site/guides.mjs` or the homepage header/footer, then `node site/check.mjs`. Four generated guides cover installation, feature usage, settings and troubleshooting. All five pages stay noindex. Serve `site/website/` to preview them; generated guide HTML must be rebuilt after source changes.

Version distinctions were checked against `main` (`ab9aafd`) and `update3` (`dc51b16`), including recipe state, preferences, settings resources and mouse controls. Update 2 has one keyboard-controlled recipe pin. Update 3 adds three pins, double-click controls, remembered local-world selections and a Personal Markers compass. Each download card links to its Windows x64 ZIP through the personal-site Worker and to the corresponding GitHub release notes. The Update 3 card uses `v1.1.2`. The five Codex carousel slides use supplied Update 3 game captures encoded as WebP. The stable card points to the available Update 2 release.

An Update 2 single-recipe capture would help illustrate the version difference. Do not fabricate screenshots of the app.

The homepage search title describes the timer, compass and recipe tracker; the visible headline keeps the approved wording. Shared metadata supplies canonical URLs, page-specific Open Graph and Twitter titles/descriptions, an in-game preview image, factual SoftwareApplication data and guide breadcrumbs. No ratings or reviews are claimed. The full in-game captures use smaller WebP display images with full-size PNG links. Run `site/prepare-display-images.py` with Pillow to regenerate the display files. All screenshot placements declare their dimensions to reserve space before loading. Metadata, image budgets and packaging are checked by the local tests.

## Publish package

Run `node site/package.mjs` to create noindex output in `dist/site/`, or add `--production` to build indexable output with canonical URLs and a sitemap for `https://ahjephson.dev/star-rupture-overlay/`. Neither command uploads files. `node --test site/package.test.mjs` checks that unused/private images and stale output are excluded. `dist/site-manifest.json` records every packaged path and its SHA-256; the manifest itself is not published.

The builder starts with the five pages and includes only their referenced styles and images. It never copies the site directory wholesale. Private originals, unused captures, duplicate reference images, scripts and the screenshot catalogue are excluded. Full in-game overview and arrival captures remain public assets because the pages show and link them. Settings/tray images use the privacy-cleaned crops. Timer and compass close-ups publish only cropped pixels.

The two silent H.264 demos show recipe tracking (20 seconds, with amber double-click indicators) and setting and following a Personal Marker (22.2 seconds). Their 1080p web copies and WebP posters live in `website/assets/video/` and publish with the product site to GitHub Pages. Each player has native controls, inline playback and `preload="none"`; neither autoplays. Text descriptions explain the actions without relying on audio. Master recordings, unedited takes and rendering files stay outside the publish package. The personal-site proxy must forward `Range` and `If-Range` and preserve partial-response headers for video seeking. Release that proxy change through the existing infrastructure workflow when publishing the demos. No additional hosting service or credentials are required.

## Manual publishing

The `Product website` workflow in `.github/workflows/website.yml` runs only when manually dispatched. Its `publish` input defaults to false: that builds, tests and saves a Pages artifact without deploying it. Pushes, tags and application releases do not publish the site.

At launch, commit the reviewed website and workflow, enable GitHub Actions as the repository's Pages source, and allow the chosen branch in the `github-pages` environment. Get the site owner’s explicit approval for the reviewed changes before enabling `publish`. Confirm the product page, guides and assets at `https://ahjephson.github.io/star-rupture-overlay/` before releasing the proxy.

The separate personal-site Worker mounts `/star-rupture-overlay/` at the GitHub Pages origin. Its source lives in `ahjephson/ahjephson.dev`; its production configuration and manual release workflow live in `lantean-code/infrastructure`. Release that Worker through infrastructure after the Pages origin is working. No new DNS record or Cloudflare asset upload is needed for this product site. Verify the public product page, guide navigation, image downloads, sitemap and missing-page status after launch.

For rollback, rerun the website workflow on the previous reviewed commit and release the previous personal-site Worker commit through infrastructure. Do not change application release tags or repository visibility as part of website publication.
