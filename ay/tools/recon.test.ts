import { expect, test } from "bun:test";
import { declared, isSitemap, locs, section } from "./recon.ts";

test("sitemap index vs url set", () => {
  expect(locs(`<sitemapindex><sitemap><loc>https://a/s1.xml</loc></sitemap></sitemapindex>`)).toEqual({ sitemaps: ["https://a/s1.xml"], pages: [] });
  expect(locs(`<urlset><url><loc> https://a/x?a=1&amp;b=2 </loc></url></urlset>`)).toEqual({ sitemaps: [], pages: ["https://a/x?a=1&b=2"] });
});

test("section is the first path segment", () => {
  expect(section("https://apify.com/store/categories")).toBe("/store");
  expect(section("https://apify.com/")).toBe("/");
});

test("declared sitemaps come from robots.txt and <link rel=sitemap>", () => {
  const home = `<head><link rel="sitemap" href="/sitemap-index.xml"/><link rel="icon" href="/x.png"></head>`;
  expect(declared("https://a.dev/", "User-agent: *\nSitemap: https://a.dev/s.xml\n", home)).toEqual(["https://a.dev/s.xml", "https://a.dev/sitemap-index.xml"]);
  expect(declared("https://a.dev/", "", "<html></html>")).toEqual([]);
});

test("an HTML page served with 200 is not a sitemap", () => {
  expect(isSitemap("<!DOCTYPE html><html><title>404</title></html>")).toBe(false);
  expect(isSitemap(`<?xml version="1.0"?><urlset xmlns="x"><url><loc>https://a/</loc></url></urlset>`)).toBe(true);
});
