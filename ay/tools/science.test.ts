import { expect, test } from "bun:test";
import { amdahl, erlangB, erlangC, examples, fitUsl, gustafson, little, mg1, mm1k, mmc, usl, uslPeakN } from "./science";

const close = (a: number, b: number, eps = 1e-9) => expect(Math.abs(a - b)).toBeLessThan(eps);

test("Little's law", () => close(little(4000, 0.005), 20));
test("Amdahl and Gustafson", () => {
  close(amdahl(0.95, 64), 15.4216867469, 1e-6);
  close(gustafson(0.05, 64), 60.85);
});
test("USL peak at sqrt((1-sigma)/kappa)", () => {
  const n = uslPeakN(0.05, 0.001);
  expect(usl(n, 0.05, 0.001)).toBeGreaterThan(usl(n - 1, 0.05, 0.001));
  expect(usl(n, 0.05, 0.001)).toBeGreaterThan(usl(n + 1, 0.05, 0.001));
});
test("USL fit recovers exact parameters", () => {
  const pts: [number, number][] = [1, 2, 4, 8, 16, 32].map((n) => [n, 1000 * usl(n, 0.03, 0.0005)]);
  const f = fitUsl(pts);
  close(f.sigma, 0.03, 1e-9);
  close(f.kappa, 0.0005, 1e-9);
});
test("M/M/1/K reduces to M/M/1 for large K, and Little holds on the admitted stream", () => {
  const r = mm1k(400, 500, 512);
  close(r.w, 1 / (500 - 400), 1e-9);
  close(r.l, r.lambdaEff * r.w, 1e-9);
});
test("M/M/1/K at rho = 1.2 blocks about (rho-1)/rho", () => close(mm1k(6000, 5000, 128).pBlock, 1 / 6, 1e-6));
test("Pollaczek-Khinchine: cs2 = 1 equals M/M/1", () => close(mg1(400, 0.002, 1).w, 0.01, 1e-12));
test("Erlang C with c = 1 equals rho; Erlang B known value", () => {
  close(erlangC(1, 0.8), 0.8);
  close(erlangB(2, 1), 0.2);
});
test("M/M/c with c = 1 equals M/M/1", () => close(mmc(400, 500, 1).w, 0.01, 1e-12));
test("worked examples are stable", () => {
  const e = examples();
  close(e.tokioServe.pWait, 0.4576, 1e-4);
  close(e.bunServe.overload.pBlock, 1 / 6, 1e-6);
});
