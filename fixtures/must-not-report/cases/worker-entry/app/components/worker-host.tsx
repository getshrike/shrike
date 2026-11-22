"use client";

import { useEffect } from "react";

export function WorkerHost() {
  useEffect(() => {
    const worker = new Worker(new URL("./heavy-worker.ts", import.meta.url), {
      type: "module",
    });
    worker.onmessage = () => worker.terminate();

    void navigator.serviceWorker.register(
      new URL("./service-worker.ts", import.meta.url),
    );

    const context = new AudioContext();
    void context.audioWorklet.addModule(
      new URL("./processor.ts", import.meta.url),
    );
  }, []);

  return <div>worker host</div>;
}
