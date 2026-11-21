export async function load() {
  const [{ used }] = await Promise.all([import("./mod")]);
  return used;
}
