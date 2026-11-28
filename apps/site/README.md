# Shrike site

Astro 7 and Tailwind CSS 4 app for the public Shrike landing site. Consumer
documentation lives in [`../docs`](../docs) (Mintlify); contributor
documentation stays in the root [`docs/`](../../docs) tree.

From the repository root, with pnpm 12.18.0:

```bash
pnpm --filter @shrike/site dev
pnpm --filter @shrike/site typecheck
pnpm --filter @shrike/site lint
pnpm --filter @shrike/site build
```

Or run those scripts from this directory with `pnpm dev`, `pnpm typecheck`,
and so on.

## Deployment

The site is a fully prerendered Astro app deployed as a Workers static assets
Worker named `getshrike-site`. Production deploys run in Cloudflare Workers
Builds from the GitHub repository when a push to `main` changes the site or
its build inputs:

```text
build command:      pnpm install --frozen-lockfile && pnpm --filter @shrike/site build
deploy command:     pnpm --filter @shrike/site run deploy
root directory:     /
build output:       apps/site/dist
```

Cloudflare Workers Builds watches these paths:

- `apps/site/*`
- `pnpm-lock.yaml`
- `pnpm-workspace.yaml`
- `package.json`

Changes outside those paths, such as docs or other packages, do not trigger a
site deploy. Cloudflare still builds on empty pushes and very large pushes
(3,000+ files or 20+ commits).

Cloudflare fetches the committed source, builds it, and publishes the output;
there is no local upload in the normal path. `wrangler.jsonc` defines the
Worker name, static asset directory, and the `getshrike.dev` and
`www.getshrike.dev` custom domains. Cloudflare creates the DNS records and TLS
certificates for those hostnames, so pre-existing A/AAAA/CNAME records for
them must be removed first.

To deploy a local build manually, run the build command above and then
`pnpm --filter @shrike/site run deploy`. Build output and generated files live in
ignored `dist/`, `.astro/`, and `.wrangler/` directories.

### Canonical host and HTTPS

`getshrike.dev` is the canonical origin. The Cloudflare zone keeps both
`getshrike.dev` and `www.getshrike.dev` attached to the Worker, and a Single
Redirect rule in the `http_request_dynamic_redirect` phase sends every
`www.getshrike.dev` request to the same path and query on
`https://getshrike.dev` with a 301. The zone also enables Always Use HTTPS and
HSTS (`max-age=31536000; includeSubDomains`), so `http://` requests redirect
before they reach the Worker and browsers pin HTTPS for the apex and
subdomains.

## Structure

```text
src/
  pages/index.astro        landing page
  layouts/BaseLayout.astro document shell, fonts, metadata
  components/              announcement bar, header, hero, feature scroll, CTA
  assets/illustrations/    SVG illustration assets
  styles/global.css        Tailwind entry, fonts, hero, terminal styles
public/
  brand/                   Shrike brand assets (see public/brand/README.md)
  fonts/                   Overused Grotesk and Geist Mono
  images/                  terminal and CTA backdrops
  patterns/                hero background pattern
```

## Fonts

The site self-hosts the variable [Overused Grotesk](https://github.com/RandomMaerks/Overused-Grotesk)
and the variable Geist Mono. Overused Grotesk is a free, SIL OFL 1.1
neo-grotesque used in place of the commercial PP Neue Montreal; at the hero's
60px size its two display lines measure within about 5px of PP Neue Montreal,
while Inter, General Sans, and Aspekta are 30–100px wider. The license is
included at `public/fonts/OverusedGrotesk-OFL.txt`. Both fonts are served from
`/fonts` and declared in `src/styles/global.css`.

## Hero

The hero is a 1:1 layout and motion study of the
[opensre.com](https://www.opensre.com/) hero: 60px display type at the `xl`
breakpoint, `-0.04em` tracking, the sequential blur-and-rise word reveal, the
install command with copy feedback, and the bordered preview frame. The copy is
Shrike's, and the install command is the documented npm install.

## Header

The sticky header follows the surface behind it. Any section can declare
`data-header-surface="dark"`; when it reaches the header's bottom edge, the
header switches its background, border, links, buttons, and logo to the dark
palette, using OpenSRE's 300ms `cubic-bezier(0.4, 0, 0.2, 1)` transition. The
feature scroll and the closing CTA are the dark surfaces today.

## Feature scroll

Directly below the hero, a scroll-driven feature section combines the sticky
table of contents from [OpenSRE](https://www.opensre.com/) with the numbered
two-column layout of [Polar's Charge section](https://polar.sh/#charge). The
five Shrike features are Measure, Find, Clean, Prove, and Protect.

### How it works intro

Above the numbered features, a three-card row adapts Polar's usage-billing
cards: a square media tile, a 20px heading, and 16px `#858585` body copy, with
no "Learn more" link. The line-burst canvas is replaced by a silver orb
inspired by ElevenLabs' ElevenCreative carousel: a static gradient texture is
sampled through UVs displaced by animated FBM noise, so the gradient itself
flows and marbles in place instead of spinning. Each shape gets its own
mapping: the square uses a planar chrome shear, the circle keeps the
reference's sphere warp, and the triangle fans the texture out of its apex like
a prism. The three orbs share the silver palette sampled from the ElevenAPI dot
and the terminal/CTA backdrop, and clip to a square, a circle, and a triangle.
Each orb is centered at 58% of its media tile, and the cards sit on the same
`#090909` surface and reveal with the feature sections.

`HowItWorksOrb.astro` owns the orb shapes, palettes, grain, shaders, and motion.
Textures are generated at 1024px with mipmaps, and the canvas renders at up to
2x device pixel ratio for sharper flow detail. On WebGL2 with
`EXT_color_buffer_float`, each orb runs a 2D stable-fluids simulation: pointer
movement injects a velocity impulse, the velocity field advects itself, a
divergence pass and a 30-iteration Jacobi pressure solve remove compression,
vorticity confinement turns the pointer's shear into a coherent eddy, and the
projected velocity advects a 512² RGBA16F dye field. The dye carries the silver
FBM pattern, so the pointer stirs the liquid itself rather than distorting a
refraction layer above it. The impulse is a narrow gaussian about the size of
the cursor, and the pattern source re-seeds faster once the pointer stops, so
the stroke stays local and the original flow returns within a couple of
seconds. The simulation advances in fixed 1/60 s substeps (up to six per frame)
so the stir stays real-time when frames drop. The display shader
lights the dye from its own luminance gradient and applies the planar, sphere,
or prism mapping. The simulation pauses when the orb leaves the viewport and
draws a single static frame under `prefers-reduced-motion`. WebGL1 browsers keep
the texture and tilt interaction through the pattern shader without the fluid
field, and if WebGL is unavailable a CSS gradient with the same palette remains
underneath the canvas. The card copy lives in `FeatureScroll.astro`.

- A 180px sticky rail holds the feature links; the active item turns white and
  gets the same small triangle marker as OpenSRE. The rail is hidden below
  `lg`, where the numbered labels carry the orientation.
- The rail reads each feature's `theme` (`dark` or `light`) from
  `data-nav-theme` and switches its active, inactive, hover, and label colors
  to stay legible over the background behind it, using OpenSRE's 200ms
  `cubic-bezier(0.4, 0, 0.2, 1)` color transition. All five features are
  currently `dark`; set a feature's `theme` to `light` when its background
  becomes light.
- Each feature uses Polar's grid: `01` and the feature name in the left column,
  the 30px headline and muted subtitle in the right, then the body copy, then a
  full-width preview.
- The preview is the same Polar texture used by the hero terminal, washed with
  `bg-black/20`, with an inset `#0A0A0A` terminal. Each terminal
  types its own Shrike command and reveals a scripted result: dead-weight
  report, orphan delta, verified batches, witness/receipt, and build evidence.
- The section is `#090909` with `#858585` supporting text and 1px `white/10`
  rules. It uses the site's Overused Grotesk and Geist Mono; no reference font
  is loaded.

`src/components/FeatureScroll.astro` owns the rail, scroll spy, reveal
animation, and terminal scripts. `FeatureTerminal.astro` owns the shared
terminal chrome and textured preview.

## Terminal preview

The preview is a live terminal, not an image. At `xl` the frame is 1304×680 and
follows the terminal composition on [react.doctor](https://www.react.doctor/):
traffic lights and a `Terminal - WxH` label, a tab strip whose label swaps
`~` → `report` → `clean`, and a screen that auto-scrolls only through script.
Wheel and touch scrolling over the terminal pass through to the page. The terminal panel is inset
48px on the top and sides and 32px at the bottom, with 12px corners, and the
screen's rendered size feeds the label through a `ResizeObserver`.

The loop is a scripted Shrike session:

1. Type `npx getshrike report`, then swap the tab to `report`.
2. Count the scanned files up to 2,184 and reveal the dead-weight report with
   blur-and-rise line transitions while the screen auto-scrolls.
3. Type `npx shrike clean --all` under the `clean` tab and reveal the baseline
   checks, three verified batches, a diff hunk, the receipt, and the success
   card.
4. Hold, fade the screen, and replay.

The animation uses the Web Animations API and `requestAnimationFrame`; there is
no client framework. It starts when the preview scrolls into view and respects
`prefers-reduced-motion`, which renders the static report without a blinking
cursor or loop. The session is a styled, monochrome composite in the
react.doctor tradition, not a literal capture: the real plain report adds a
header, a Unicode bar, and a cyan accent on the dead-weight figure and filled
bar, while the interactive TUI is an Ink view with selectable rows. Content is
illustrative but uses the report numbers from the root README. Vanilla CSS in
`src/styles/global.css` and one inline script in `src/components/Hero.astro`
own the behavior.

The backdrop is Polar's 1280×720 `Polar_Flow_03_No_Logo.jpg` artwork, upscaled
to 2560×1440 and stored at `public/images/terminal-backdrop.jpg` (≈329 KB) and
covered by a `bg-black/20` wash. The logo-free source is used everywhere so no
Polar wordmark appears in the terminal or the CTA. Polar uses the flow layer at
`bg-black/40` behind a 608px panel; the lighter wash keeps the texture readable
around the wider terminal. It is third-party artwork used for the reference;
confirm permission or replace it before publishing.

The motion was reverse engineered with the `rea` CLI and Playwright inspection
of the two reference pages. The relevant findings:

- react.doctor: terminal `640×586` at a 1440 viewport, `w-160` (640px),
  `h-146.5` (586px), `rounded-3xl`; a 45px chrome row, a 38px tab strip, and a
  single scrolling content column. Lines arrive with a blur-and-rise transition
  (`opacity` and `filter: blur`, up to about 4.5px, over roughly 500ms) and the
  column scrolls smoothly rather than jumping.
- polar.sh: the live flow image is a 1280×720 JPEG in an absolutely positioned
  `object-cover` layer, not a canvas or video. Polar also ships a 1920×1080
  `landing/company/polar.jpg` with the wordmark; the site uses the logo-free
  `Polar_Flow_03_No_Logo.jpg` instead. Polar places the image in a rounded,
  overflow-hidden container with a `bg-black/40` overlay and a smaller rounded
  panel on top, so the artwork reads as a texture behind the panel rather than
  as a full-bleed hero image.

## Call to action

The closing section mirrors the OpenSRE CTA: a wide, bottom-aligned banner
(`440px`, `400px` at `sm`, `494px` at `lg`) with the headline on the left and the
install box on the right. The box copies `npx getshrike report`; the `npx` and
`npm` tabs switch between the quick run and the documented project install, and
the copy button swaps to a check for 1.5s. The section is `#090909`, uses the
same `data-header-surface="dark"` hook as the feature scroll, and its background
is a 2880×968 wide crop of the same Polar artwork at
`public/images/cta-backdrop.jpg`.

