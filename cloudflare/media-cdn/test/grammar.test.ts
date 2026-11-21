import { describe, expect, test } from "bun:test";
import { GrammarError, formatImage, negotiate, parseImage, parseVideoFile, variantOf, videoFile } from "../../../tools/media-cdn/grammar.ts";
import { sourcesOf } from "../../../tools/media-cdn/manifest.ts";
import { manifest } from "./env.ts";

describe("image grammar", () => {
  test("name, pin, format and tokens are told apart", () => {
    expect(parseImage("hero")).toEqual({ name: "hero" });
    expect(parseImage("apps/antigravity.3f9a2c1d.webp=s144")).toEqual({ name: "apps/antigravity", pin: "3f9a2c1d", format: "webp", square: 144 });
    expect(parseImage("hero.avif=w1440-h600-q70")).toEqual({ name: "hero", format: "avif", width: 1440, height: 600, quality: 70 });
    expect(parseImage("photo.jpeg=w600").format).toBe("jpg");
  });

  test("a name that ends in eight hex digits without a dot is not a pin", () => {
    expect(parseImage("deadbeef")).toEqual({ name: "deadbeef" });
    expect(parseImage("logo-cafebabe.avif")).toEqual({ name: "logo-cafebabe", format: "avif" });
  });

  test("canonical text is independent of token order", () => {
    expect(formatImage(parseImage("hero.avif=q70-h600-w1440"))).toBe("hero.avif=w1440-h600-q70");
  });

  test.each(["Hero", "a//b", "-x", "hero=w15", "hero=w4097", "hero=w600-s144", "hero=q10", "hero=z5", "hero=w6x", "../x"])("%s is refused", (path) => {
    expect(() => parseImage(path)).toThrow(GrammarError);
  });

  test("only exact single rungs are prebuilt candidates", () => {
    expect(variantOf(parseImage("a=w600"))).toBe("w600");
    expect(variantOf(parseImage("a=s48"))).toBe("s48");
    expect(variantOf(parseImage("a=w600-h300"))).toBeUndefined();
    expect(variantOf(parseImage("a=w600-q70"))).toBeUndefined();
  });

  test("negotiation prefers AVIF, then WebP, then the fallback", () => {
    expect(negotiate("image/avif,image/webp,*/*", "jpg")).toBe("avif");
    expect(negotiate("image/webp,*/*", "png")).toBe("webp");
    expect(negotiate("*/*", "png")).toBe("png");
    expect(negotiate(null, "jpg")).toBe("jpg");
  });
});

describe("video grammar", () => {
  test("a rung file name round-trips and codec and container must agree", () => {
    expect(parseVideoFile("1920w-2x-av1.mp4")).toEqual({ width: 1920, density: 2, codec: "av1" });
    expect(videoFile(600, 1, "vp9")).toBe("600w-1x-vp9.webm");
    expect(parseVideoFile("600w-1x-vp9.mp4")).toBeUndefined();
    expect(parseVideoFile("../x.mp4")).toBeUndefined();
  });

  test("sources follow the measured breakpoints, AV1 2x then VP9 2x then H.264 1x, with codec-qualified types", () => {
    const rows = sourcesOf("test-wide", manifest.video["test-wide"]!, "https://media.example");
    expect(rows).toHaveLength(12);
    expect(rows.slice(0, 3).map((r) => [r.media, r.type.split(";")[0], r.src.split("/").pop()])).toEqual([
      ["(max-width: 599px)", "video/mp4", "600w-2x-av1.mp4"],
      ["(max-width: 599px)", "video/webm", "600w-2x-vp9.webm"],
      ["(max-width: 599px)", "video/mp4", "600w-1x-h264.mp4"],
    ]);
    expect(rows[0]!.type).toBe('video/mp4; codecs="av01.0.08M.08"');
    expect(rows[3]!.media).toBe("(min-width: 600px) and (max-width: 1023px)");
    expect(rows[6]!.media).toBe("(min-width: 1440px)");
    expect(rows[9]!.media).toBeUndefined();
    expect(rows[9]!.src).toBe("https://media.example/v1/video/test-wide.cccc3333/1440w-2x-av1.mp4");
  });
});
