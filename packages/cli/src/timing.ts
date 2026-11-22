/** Opt-in phase timing for the CLI, matching the engine's SHRIKE_TIMING. */
export function timingEnabled(): boolean {
  return Boolean(process.env.SHRIKE_TIMING);
}

export function logTiming(name: string, start: number): void {
  if (timingEnabled()) {
    console.error(`shrike cli: ${name}=${Math.round(performance.now() - start)}ms`);
  }
}
