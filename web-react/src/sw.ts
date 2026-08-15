// Registers the service worker and forwards its "update-available" message.
//
// The worker is registered only in a production build: during development the
// dev server's hot reload and a caching worker would fight over the same files.
// Offline use is optional, so a failed registration is ignored rather than
// surfaced.

export function registerServiceWorker(onUpdate: () => void): void {
  if (!import.meta.env.PROD) return;
  if (!("serviceWorker" in navigator)) return;

  navigator.serviceWorker.register("sw.js").catch(() => {
    /* offline use is optional */
  });
  navigator.serviceWorker.addEventListener("message", (event) => {
    const data = event.data as { type?: string } | null;
    if (data?.type !== "update-available") return;
    onUpdate();
  });
}
