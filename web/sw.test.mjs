// Tests for the service worker, run with `node web/sw.test.mjs`.
//
// The page's *rendering* cannot be tested without a headless browser or `jsdom`, and this
// repository has no external dependencies. The service worker can: its whole surface is
// `Cache`, `fetch` and `postMessage`, and Node has `Response` and `fetch` built in, so a fake
// Cache and a switchable network are enough to drive every path.
//
// That matters more than it might for a static site. A cached tax calculator serves the law it
// was built against, and the paths below are what stop it doing so silently.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import assert from "node:assert/strict";

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(join(here, "sw.js"), "utf8");

const listeners = {};
let messages = [];
let store = new Map();
let online = true;
let body = "OLD";

globalThis.self = {
  addEventListener: (type, handler) => { listeners[type] = handler; },
  clients: {
    claim: async () => {},
    matchAll: async () => [{ postMessage: message => messages.push(message) }],
  },
};
const cache = {
  match: async request => store.get(String(request)),
  put: async (request, response) => { store.set(String(request), response); },
  addAll: async () => {},
};
globalThis.caches = {
  open: async () => cache,
  keys: async () => [],
  delete: async () => {},
};
globalThis.fetch = async () => {
  if (!online) throw new Error("offline");
  return new Response(body, { status: 200 });
};

// The worker is a script, not a module; append an export so its internals can be driven.
const sw = await import("data:text/javascript;base64," +
  Buffer.from(`${source}\nexport { serve };`).toString("base64"));

/// Lets the background revalidation settle.
const settle = () => new Promise(resolve => setTimeout(resolve, 20));

const reset = (cached = null) => {
  store = cached ? new Map([["./index.html", new Response(cached, { status: 200 })]]) : new Map();
  messages = [];
  online = true;
  body = "OLD";
};

let failures = 0;
async function test(name, run) {
  try {
    reset();
    await run();
    console.log(`  ok  ${name}`);
  } catch (error) {
    failures += 1;
    console.error(`  FAIL ${name}\n       ${error.message}`);
  }
}

console.log("service worker");

await test("a cold request fetches and caches", async () => {
  const response = await sw.serve("./index.html");
  assert.equal(await response.text(), "OLD");
  assert.equal(store.size, 1, "the response should have been cached");
});

await test("a cached request is served without notifying", async () => {
  reset("OLD");
  const response = await sw.serve("./index.html");
  assert.equal(await response.text(), "OLD");
  await settle();
  assert.equal(messages.length, 0, "an unchanged build must not claim an update");
});

await test("a changed build serves the cached copy first, then notifies", async () => {
  reset("OLD");
  body = "NEW";
  const response = await sw.serve("./index.html");
  // The point: a reader mid-calculation keeps the build they started with, so one table
  // cannot show figures from two statutory datasets.
  assert.equal(await response.text(), "OLD", "must not swap the build underneath the page");
  await settle();
  assert.equal(messages.length, 1);
  assert.equal(messages[0].type, "update-available");
});

await test("offline with nothing cached answers rather than failing", async () => {
  online = false;
  const response = await sw.serve("./missing");
  assert.equal(response.status, 503);
  assert.match(await response.text(), /Offline/);
});

await test("offline with a cached copy serves it", async () => {
  reset("CACHED");
  online = false;
  const response = await sw.serve("./index.html");
  assert.equal(await response.text(), "CACHED");
  await settle();
  assert.equal(messages.length, 0, "an unreachable network is not an update");
});

await test("the install handler caches the whole shell", async () => {
  const cached = [];
  cache.addAll = async files => cached.push(...files);
  await listeners.install({ waitUntil: promise => promise });
  await settle();
  for (const file of ["./index.html", "./casivell_wasm.wasm", "./manifest.webmanifest"]) {
    assert.ok(cached.includes(file), `${file} must be cached for offline use`);
  }
});

console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
