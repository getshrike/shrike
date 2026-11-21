export async function load() {
  const used = await import("./mod");
  const escaped = await import("./escaped");
  let reassigned = await import("./reassigned");
  use(escaped);
  return used.used + (reassigned.epsilon ? 1 : 0);
}

function use(value: unknown) {
  return value ? 1 : 0;
}
