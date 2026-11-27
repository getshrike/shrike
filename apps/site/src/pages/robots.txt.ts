import type { APIRoute } from "astro";

export const GET: APIRoute = ({ site }) => {
  const base = site ?? new URL("https://getshrike.dev");
  const sitemap = new URL("/sitemap-index.xml", base).href;

  return new Response(
    ["User-agent: *", "Allow: /", "", `Sitemap: ${sitemap}`, ""].join("\n"),
    {
      headers: {
        "Content-Type": "text/plain; charset=utf-8",
      },
    },
  );
};
