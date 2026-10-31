# Saytwig landing page

Buildless, repository-native HTML/CSS/JavaScript. Serve **`public/`**, not this
folder; product context and review evidence are development-only.

From the repository root, start a loopback-only preview:

```sh
python3 -m http.server 4612 --bind 127.0.0.1 --directory web/public
```

## Deploy from main

The `Deploy website` GitHub Actions workflow uploads only `web/public/` to the
existing Cloudflare Pages project `saytwig`. Pushes to `main` that change served
files or the deployment workflow trigger deployment. Pull requests, tags, and
other branches do not deploy. The workflow can also run manually on `main` to
retry a deployment. Deployments do not interrupt an active upload.

The site needs no package installation or build step. The workflow installs
only the pinned Wrangler deployment tool. Each job checks out current `main`
after it leaves the deployment queue, including reruns of older workflow runs.
It records that checked-out commit SHA in Pages and preserves `saytwig.pages.dev`
and `saytwig.goarts.win`. The deployed SHA can differ from the triggering run's
SHA if `main` advanced while the job waited.

Before the first deployment:

1. In the repository's **Settings > Secrets and variables > Actions**, add
   `CLOUDFLARE_API_TOKEN` as a repository secret. Use an owner-approved API token
   with **Account > Cloudflare Pages > Edit** for account
   `cfc4f7e12752e71d7c6677585e057436`. Do not put the token in source or chat.
2. Set the existing project's `production_branch` to `main` through Cloudflare's
   [Update Project API](https://developers.cloudflare.com/api/resources/pages/subresources/projects/methods/edit/).
   Direct Upload projects require this API change. Keep the domains, deployment
   configuration, and project unchanged.
3. Merge a website change to `main`, or run **Deploy website** on `main`.
   A missing token or a production branch other than `main` fails before upload.
4. Confirm that the Pages production deployment records the checked-out commit
   SHA and that both domains serve the files from that commit's `web/public/`.

An existing Cloudflare OAuth session is not a GitHub Actions credential.
Direct Upload projects cannot switch to Git integration. This workflow follows
Cloudflare's [Direct Upload CI guide](https://developers.cloudflare.com/pages/how-to/use-direct-upload-with-continuous-integration/)
without replacing the project. To roll back the site, revert the served-file
change on `main`. To stop automatic uploads, disable **Deploy website** in Actions.

There is no package installation, build step, analytics, external
font request, microphone request, or API call. The voice-to-text story is synthetic: one finite entrance plus scroll-linked
Voice → Words → Local scenes. Passive scroll events coalesce into at most one
pending animation frame; there is no perpetual animation loop. The system
`prefers-reduced-motion` preference provides static reading order. No-JavaScript content also stays
readable. This is not a native app screenshot or live recording.

## Assets and rights

- `public/assets/saytwig.svg`: the project's original Saytwig mark.
- `public/assets/Lato-Regular.woff2`: Lato by Łukasz Dziedzic, converted losslessly to WOFF2,
  redistributed under the accompanying SIL Open Font License (`OFL.txt`).
- The remaining diagram is original HTML/CSS geometry; no generated raster or
  third-party product screenshot is used.

The compressed font is the largest asset (212,004 bytes). The whole served directory is below
250 KB in raw file bytes. No latency or performance benchmark is claimed.

## Review

Design review applies Impeccable 4.7.0 and the current Skillbox design owners
while retaining the approved black/mint identity, logo and Lato typeface.
`PRODUCT.md` and `DESIGN.md` record the product and design boundaries. Manual
Chromium inspection covered desktop 1440×900, 768×600, and narrow 390px and
320px viewports. Physical phones remain unverified.

The landing links to source. App-only nightly artifacts are build outputs,
not an accepted installer or proof of Windows capture, paste or update behavior.

### Font provenance verification

The full license is retained. The original TTF was verified before lossless WOFF2 conversion with FontTools 4.61.1. Glyph order, Unicode mapping, advance metrics and glyph coordinates match the source.
On 2026-10-30 the source font and license Git blob IDs matched the primary
[Google Fonts Lato directory](https://github.com/google/fonts/tree/main/ofl/lato)
returned by GitHub's contents API:

| File | Git blob ID | SHA-256 of source bytes |
| --- | --- | --- |
| Source Lato-Regular.ttf | `0f3d0f837d24834b9b5b0a6b735459c56f5e75c3` | `d636e4683231f931eda222d588e944d082bfd3bdba02f928bee461c0f185b251` |
| OFL.txt | `98383e3d835a5c5aea988a74b569193a30dcbcec` | `74ba064d03f1f1c4a952da936c3eb71866c34404916734de3cae73b34357e59e` |

This verifies an exact upstream content match, not an undocumented historical
fetch commit. These website assets are not bundled into the native app.

The served WOFF2 is 212,004 bytes with SHA-256 `d414a00ed3e47a8cc7462bd43975f0214a05c10b70b5f22686430beb6057ae2c`. The format conversion preserves the typeface and its SIL Open Font License.

The HTML pins CSS and JavaScript URLs with the first 16 hex characters of each file’s SHA-256. Update the matching `?v=` value whenever either asset changes. This prevents a returning browser from combining a cached script or stylesheet with newly deployed HTML; it does not require changing zone-wide cache settings.
