export async function load() {
  const used = await import("./mod").then((module) => module.used);
  const escaped = await import("./escaped").then((module) => use(module));
  return used + (escaped ? 1 : 0);
}

function use(value: unknown) {
  return value ? 1 : 0;
}
