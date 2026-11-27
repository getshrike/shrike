/**
 * Cloudflare Workers deployment for the static Shrike landing site.
 *
 * `cf deploy ./dist` uploads the prebuilt Astro output as an assets-only Worker
 * and uses these Worker settings, including the production custom domain.
 */
export default {
  worker: {
    name: "getshrike-site",
    compatibilityDate: "2026-11-27",
    domains: ["getshrike.dev", "www.getshrike.dev"],
  },
};
