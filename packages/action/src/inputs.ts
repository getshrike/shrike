import { appendFileSync, existsSync, readFileSync } from "node:fs";

export function input(name: string, fallback = ""): string {
  return process.env[`INPUT_${name.toUpperCase().replace(/-/g, "_")}`] ?? fallback;
}

export function writeOutput(name: string, value: string): void {
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `${name}=${value}\n`);
}

export function writeSummary(text: string): void {
  const path = process.env.GITHUB_STEP_SUMMARY;
  if (path) appendFileSync(path, `${text}\n`);
}

/**
 * Refuse write-capable flows on a fork PR. The event file is authoritative; a
 * missing or unreadable event on a PR fails closed.
 */
export function isForkPullRequest(): boolean {
  const eventName = process.env.GITHUB_EVENT_NAME ?? "";
  if (!eventName.startsWith("pull_request")) return false;
  const path = process.env.GITHUB_EVENT_PATH;
  if (!path || !existsSync(path)) return true;
  try {
    const event = JSON.parse(readFileSync(path, "utf8")) as {
      pull_request?: { head?: { repo?: { full_name?: string } } };
      repository?: { full_name?: string };
    };
    const head = event.pull_request?.head?.repo?.full_name;
    const base = event.repository?.full_name;
    return typeof head === "string" && typeof base === "string" && head !== base;
  } catch {
    return true;
  }
}
