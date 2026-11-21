const state = { frames: 0 };

export function handle(message: string) {
  state.frames += 1;
  return `${message}:${state.frames}`;
}

self.onmessage = (event) => {
  handle(event.data);
};

function unusedWorkerHelper() {
  return 1;
}
