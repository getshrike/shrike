const cacheName = "worker-entry-v1";

self.addEventListener("install", () => {
  void caches.open(cacheName);
});

function unusedServiceWorkerHelper() {
  return 1;
}
