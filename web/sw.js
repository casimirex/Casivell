// Casivell's service worker.
//
// # The hazard this is written around
//
// An offline tax calculator that serves stale statutory data is worse than one that does not
// work offline at all: it answers confidently with last year's rates and nothing on the page
// looks wrong. Caching a calculator is not like caching a blog.
//
// So the strategy is **stale-while-revalidate**, not cache-first: every load serves the cached
// shell immediately and fetches a fresh copy in the background. When the fresh copy differs,
// the page is told, and it says so rather than swapping figures underneath a reader who is
// mid-calculation.
//
// The second guard is not here at all — it is the Datenstand in the page footer, which names
// the statutory data the figures rest on. A cached build shows the digest it was built with,
// so a stale answer is identifiable rather than merely suspected.

const VERSION = "v1";
const CACHE = `casivell-${VERSION}`;

// The whole application. Two files and a manifest — there is no framework to cache.
const SHELL = ["./", "./index.html", "./casivell_wasm.wasm", "./manifest.webmanifest"];

self.addEventListener("install", event => {
  // No `skipWaiting`: a running page keeps the build it started with. Swapping the wasm under
  // a calculation in progress could show figures from two different statutory datasets in one
  // table, which is precisely the confusion the Datenstand exists to prevent.
  event.waitUntil(caches.open(CACHE).then(cache => cache.addAll(SHELL)));
});

self.addEventListener("activate", event => {
  event.waitUntil(
    caches.keys().then(names =>
      Promise.all(names.filter(name => name !== CACHE).map(name => caches.delete(name))))
      .then(() => self.clients.claim()));
});

self.addEventListener("fetch", event => {
  const { request } = event;
  if (request.method !== "GET") return;
  event.respondWith(serve(request));
});

async function serve(request) {
  const cache = await caches.open(CACHE);
  const cached = await cache.match(request, { ignoreSearch: true });

  if (cached) {
    // Serve at once, then check for a newer build behind it. The cached Response is handed
    // to the page, so the comparison uses a clone — a body can only be read once, and
    // reading the one we returned would break the page rather than the check.
    const forComparison = cached.clone();
    revalidate(cache, request, forComparison);
    return cached;
  }

  // Nothing cached: the network is the only source, and a navigation still needs an answer
  // if it is unreachable.
  try {
    const response = await fetch(request);
    if (response.ok) cache.put(request, response.clone());
    return response;
  } catch {
    return (await cache.match("./index.html"))
      ?? new Response("Offline und nicht zwischengespeichert.", {
        status: 503,
        headers: { "content-type": "text/plain; charset=utf-8" },
      });
  }
}

/// Fetches a fresh copy, stores it, and tells the pages if it differs from what was served.
///
/// Compared by body rather than by header: a static host's `ETag` and `Last-Modified` are not
/// reliable across the range of places this might be served from, and a false "update
/// available" is worse than reading a few tens of kilobytes.
async function revalidate(cache, request, previous) {
  let response;
  try {
    response = await fetch(request);
  } catch {
    return; // The offline case, which is not an error.
  }
  if (!response.ok) return;

  const copy = response.clone();
  await cache.put(request, response);

  const [before, after] = await Promise.all([previous.text(), copy.text()]);
  if (before === after) return;
  for (const client of await self.clients.matchAll()) {
    client.postMessage({ type: "update-available" });
  }
}
