// Declared public API in shrike.config.json. These exports are protected even
// though no module in the fixture imports them.
export function publicWidget(): string {
  return "public";
}

export function documentedHelper(): string {
  return "documented";
}
