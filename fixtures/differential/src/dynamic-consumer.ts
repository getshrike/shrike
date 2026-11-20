export async function loadDynamic(): Promise<string> {
  // Destructuring names the only export this module consumes. The sibling
  // export must still be reported as dead.
  const { usedByDynamic } = await import("./dynamic");
  return usedByDynamic();
}
