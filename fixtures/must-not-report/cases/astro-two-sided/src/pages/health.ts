import type { APIRoute } from "astro";

export const GET: APIRoute = () => new Response("ok");
export const unusedEndpointHelper = 1;
