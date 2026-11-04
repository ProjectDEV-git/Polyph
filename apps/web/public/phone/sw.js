// The phone app's service worker: opens offline from cached files (the room
// itself lives in IndexedDB) and shows approval alerts. It never caches the
// relay's traffic, which is a WebSocket the page owns.
const CACHE = "polyph-phone-v1";
const SHELL = ["./", "engine.js", "engine.wasm", "manifest.webmanifest", "icon-192.png", "icon-512.png"];

self.addEventListener("install", (e) => {
  e.waitUntil(
    caches
      .open(CACHE)
      .then((c) => c.addAll(SHELL))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (e) => {
  e.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim()),
  );
});

const put = async (req, res) => {
  if (res.ok) await (await caches.open(CACHE)).put(req, res.clone());
  return res;
};

self.addEventListener("fetch", (e) => {
  const req = e.request;
  const url = new URL(req.url);
  if (req.method !== "GET" || url.origin !== self.location.origin) return;
  if (req.mode === "navigate") {
    // Fresh pages when online, the cached app when not.
    e.respondWith(
      fetch(req)
        .then((r) => put(req, r))
        .catch(() => caches.match(req).then((r) => r ?? caches.match("./"))),
    );
    return;
  }
  if (url.pathname.includes("/_next/static/")) {
    // Content-hashed and immutable.
    e.respondWith(caches.match(req).then((r) => r ?? fetch(req).then((n) => put(req, n))));
    return;
  }
  if (url.pathname.startsWith(new URL("./", self.registration.scope).pathname)) {
    // The engine and icons: cached copy now, refreshed for next time.
    e.respondWith(
      caches.match(req).then((hit) => {
        const net = fetch(req)
          .then((n) => put(req, n))
          .catch(() => hit);
        return hit ?? net;
      }),
    );
  }
});

self.addEventListener("notificationclick", (e) => {
  e.notification.close();
  e.waitUntil(
    self.clients.matchAll({ type: "window" }).then((all) => {
      const open = all.find((c) => "focus" in c);
      return open ? open.focus() : self.clients.openWindow("./");
    }),
  );
});
