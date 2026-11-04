// The phone app's service worker: keeps the app (and its 1 MB engine)
// available offline and shows notifications. It never caches room data:
// that lives encrypted in IndexedDB, and the relay is reached directly.
const CACHE = "polyph-phone-v1";
const SHELL = ["/m/", "/manifest.webmanifest", "/icon-192.png", "/icon-512.png"];

self.addEventListener("install", (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (e) => {
  e.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (e) => {
  const url = new URL(e.request.url);
  if (e.request.method !== "GET" || url.origin !== self.location.origin) return;
  // Versioned build files never change: cache first.
  if (url.pathname.startsWith("/_next/static/") || url.pathname.endsWith(".wasm")) {
    e.respondWith(
      caches.match(e.request).then(
        (hit) =>
          hit ||
          fetch(e.request).then((r) => {
            if (r.ok) {
              const copy = r.clone();
              caches.open(CACHE).then((c) => c.put(e.request, copy));
            }
            return r;
          }),
      ),
    );
    return;
  }
  // Pages: the network when there is one, the cached shell otherwise.
  if (e.request.mode === "navigate") {
    e.respondWith(
      fetch(e.request)
        .then((r) => {
          const copy = r.clone();
          caches.open(CACHE).then((c) => c.put("/m/", copy));
          return r;
        })
        .catch(() => caches.match("/m/")),
    );
  }
});

self.addEventListener("notificationclick", (e) => {
  e.notification.close();
  e.waitUntil(
    self.clients.matchAll({ type: "window" }).then((all) => {
      const open = all.find((c) => new URL(c.url).pathname.startsWith("/m/"));
      return open ? open.focus() : self.clients.openWindow("/m/");
    }),
  );
});
