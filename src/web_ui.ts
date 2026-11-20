import { readFileSync } from "node:fs";
import { join } from "node:path";

const WEB_ROOT = join(import.meta.dir, "..", "web");

function readWebAsset(file: string): string {
  return readFileSync(join(WEB_ROOT, file), "utf8");
}

export function renderFwWebHtml(): string {
  return readWebAsset("index.html");
}

export function readFwWebAsset(pathname: string): { body: string; contentType: string } | null {
  switch (pathname) {
    case "/":
    case "/ui":
    case "/index.html":
      return { body: readWebAsset("index.html"), contentType: "text/html; charset=utf-8" };
    case "/app.css":
      return { body: readWebAsset("app.css"), contentType: "text/css; charset=utf-8" };
    case "/app.js":
      return { body: readWebAsset("app.js"), contentType: "text/javascript; charset=utf-8" };
    case "/robots.txt":
      return { body: readWebAsset("robots.txt"), contentType: "text/plain; charset=utf-8" };
    case "/sitemap.xml":
      return { body: readWebAsset("sitemap.xml"), contentType: "application/xml; charset=utf-8" };
    case "/llms.txt":
      return { body: readWebAsset("llms.txt"), contentType: "text/markdown; charset=utf-8" };
    default:
      return null;
  }
}
