import { expect, test } from "bun:test";
import { locs, section } from "./recon.ts";

test("sitemap index vs url set", () => {
  expect(locs(`<sitemapindex><sitemap><loc>https://a/s1.xml</loc></sitemap></sitemapindex>`)).toEqual({ sitemaps: ["https://a/s1.xml"], pages: [] });
  expect(locs(`<urlset><url><loc> https://a/x?a=1&amp;b=2 </loc></url></urlset>`)).toEqual({ sitemaps: [], pages: ["https://a/x?a=1&b=2"] });
});

test("section is the first path segment", () => {
  expect(section("https://apify.com/store/categories")).toBe("/store");
  expect(section("https://apify.com/")).toBe("/");
});
