// Permite abrir Notable sin conexión. Solo guarda los archivos de la página; las notas y Supabase
// no pasan por aquí. Primero intenta traer lo más nuevo y, si no hay internet, usa lo guardado.
const CACHE = "notable-web";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET" || new URL(request.url).origin !== location.origin) return;
  event.respondWith(
    fetch(request)
      .then((response) => {
        if (response.ok) {
          const copy = response.clone();
          caches.open(CACHE).then((cache) => cache.put(request, copy));
        }
        return response;
      })
      .catch(() =>
        caches.match(request).then((hit) => hit || (request.mode === "navigate" ? caches.match("./") : Response.error()))
      )
  );
});
