export default async function Page() { const mod = await import("../src/feature"); return mod.feature(); }
