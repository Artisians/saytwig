# Saytwig landing page

Buildless, repository-native HTML/CSS/JavaScript. Serve **`public/`**, not this
folder; product context and review evidence are development-only.

In the managed cloud environment:

```sh
sites-preview start /workspace/shared/saytwig-next/web
sites-preview status
sites-preview stop
```

Use the exact URL returned by the preview tool. This command runs a local
preview, not a public deployment. There is no package installation, build step, analytics, external
font request, microphone request, or API call. The voice-to-text story is synthetic: one finite entrance plus scroll-linked
Voice → Words → Local scenes. Passive scroll events coalesce into at most one
pending animation frame; there is no perpetual animation loop. A production
Motion on/off control and `prefers-reduced-motion` both provide static reading
order. System reduction cannot be overridden. No-JavaScript content also stays
readable. This is not a native app screenshot or live recording.

## Assets and rights

- `public/assets/saytwig.svg`: the project's original Saytwig mark.
- `public/assets/Lato-Regular.ttf`: unmodified Lato by Łukasz Dziedzic,
  redistributed under the accompanying SIL Open Font License (`OFL.txt`).
- The remaining diagram is original HTML/CSS geometry; no generated raster or
  third-party product screenshot is used.

The font is the largest asset (~657 KB); the whole served directory is below
700 KB in raw file bytes. No latency or performance benchmark is claimed.

## Review

The official Impeccable 4.8.0 workflow and pinned engine 0.1.18 were used, with
the coordinator confirming code-first and the existing black/mint identity.
`PRODUCT.md` and `DESIGN.md` record the product and design boundaries. Manual
browser review covered desktop and an actual 500px-wide Chromium window,
including scene navigation and the Motion control. Smaller viewports and
physical phones remain unverified. The independent finish review found no
material visual defects in those captures.

Release/download claims must stay aligned with the app repository. Source links
are deliberate: there are no installers or downloadable nightly binaries yet.

### Font provenance verification

The existing local font and full license were retained without modification.
On 2026-10-30 their computed Git blob IDs matched the primary
[Google Fonts Lato directory](https://github.com/google/fonts/tree/main/ofl/lato)
returned by GitHub's contents API:

| File | Git blob ID | SHA-256 of retained bytes |
| --- | --- | --- |
| Lato-Regular.ttf | `0f3d0f837d24834b9b5b0a6b735459c56f5e75c3` | `d636e4683231f931eda222d588e944d082bfd3bdba02f928bee461c0f185b251` |
| OFL.txt | `98383e3d835a5c5aea988a74b569193a30dcbcec` | `74ba064d03f1f1c4a952da936c3eb71866c34404916734de3cae73b34357e59e` |

This verifies an exact upstream content match, not an undocumented historical
fetch commit. These website assets are not bundled into the native app.
