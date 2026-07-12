import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { productionOutput } from "./paths.mjs";

const types = new Map([
  [".css", "text/css; charset=utf-8"],
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".svg", "image/svg+xml"],
  [".txt", "text/plain; charset=utf-8"],
  [".woff", "font/woff"],
  [".woff2", "font/woff2"]
]);

const server = http.createServer((request, response) => {
  const pathname = decodeURIComponent(new URL(request.url ?? "/", "http://localhost").pathname);
  const relative = pathname === "/" ? "index.html" : pathname.replace(/^\//, "");
  const file = path.resolve(productionOutput, relative);
  if (!file.startsWith(`${productionOutput}${path.sep}`)) {
    response.writeHead(403).end();
    return;
  }
  fs.stat(file, (error, stat) => {
    if (error || !stat.isFile()) {
      response.writeHead(404, { "Content-Type": "text/plain; charset=utf-8" }).end("Not found");
      return;
    }
    response.writeHead(200, { "Content-Type": types.get(path.extname(file)) ?? "application/octet-stream" });
    fs.createReadStream(file).pipe(response);
  });
});

server.listen(4173, "127.0.0.1", () => console.log("Documentation test server listening on http://127.0.0.1:4173"));
