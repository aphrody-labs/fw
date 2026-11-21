#!/usr/bin/env bun
// Performance laws used by docs/science: every number printed in the worked examples comes from here.
//   bun tools/science.ts            print all worked examples (inputs and results)
//   bun tools/science.ts --json     same, machine-readable
//   bun tools/science.ts fit <file> least-squares USL fit of "N X" pairs (one per line)

export const little = (lambda: number, w: number) => lambda * w; // L = lambda * W
export const amdahl = (p: number, n: number) => 1 / (1 - p + p / n);
export const gustafson = (alpha: number, n: number) => n - alpha * (n - 1);
export const usl = (n: number, sigma: number, kappa: number) => n / (1 + sigma * (n - 1) + kappa * n * (n - 1));
export const uslPeakN = (sigma: number, kappa: number) => Math.sqrt((1 - sigma) / kappa);

/** M/M/1/K: single server, K places in the system (queue plus the one in service). */
export function mm1k(lambda: number, mu: number, k: number) {
  const rho = lambda / mu;
  const pK = rho === 1 ? 1 / (k + 1) : ((1 - rho) * rho ** k) / (1 - rho ** (k + 1));
  const lambdaEff = lambda * (1 - pK);
  let l = 0;
  for (let n = 0; n <= k; n++) l += n * (rho === 1 ? 1 / (k + 1) : ((1 - rho) * rho ** n) / (1 - rho ** (k + 1)));
  const w = l / lambdaEff; // Little's law on the admitted stream
  return { rho, pBlock: pK, lambdaEff, l, w, wq: w - 1 / mu };
}

/** M/G/1 mean wait (Pollaczek-Khinchine): s = mean service time, cs2 = squared coefficient of variation of service time
 *  (0 deterministic, 1 exponential). Wq = rho s (1 + cs2) / (2 (1 - rho)). */
export function mg1(lambda: number, s: number, cs2: number) {
  const rho = lambda * s;
  const wq = (rho * s * (1 + cs2)) / (2 * (1 - rho));
  return { rho, wq, w: wq + s };
}

/** Erlang C: probability that an arrival waits in M/M/c (stable only when a = lambda/mu < c). */
export function erlangC(c: number, a: number) {
  let sum = 0;
  let term = 1;
  for (let k = 0; k < c; k++) {
    sum += term;
    term *= a / (k + 1);
  }
  const top = term * (c / (c - a)); // a^c / c! * c / (c - a)
  return top / (sum + top);
}

/** M/M/c (c workers, one shared queue, unbounded). */
export function mmc(lambda: number, mu: number, c: number) {
  const a = lambda / mu;
  const rho = a / c;
  const pWait = erlangC(c, a);
  const wq = pWait / (c * mu - lambda);
  return { a, rho, pWait, wq, w: wq + 1 / mu, lq: lambda * wq, l: lambda * (wq + 1 / mu) };
}

/** Erlang B: blocking probability of M/M/c/c, by the stable recursion. */
export function erlangB(c: number, a: number) {
  let b = 1;
  for (let k = 1; k <= c; k++) b = (a * b) / (k + a * b);
  return b;
}

/** Tail of the M/M/c waiting time: P(Wq > t) = C(c,a) * exp(-(c*mu - lambda) * t). */
export const mmcWaitTail = (lambda: number, mu: number, c: number, t: number) =>
  erlangC(c, lambda / mu) * Math.exp(-(c * mu - lambda) * t);

/** Fit sigma, kappa of the USL by linear least squares on y = N/X(N) - 1 normalised: with C(N) = X(N)/X(1),
 *  N/C - 1 = sigma (N-1) + kappa N (N-1)  (Gunther, arXiv:0808.1431). */
export function fitUsl(points: [number, number][]) {
  const x1 = points.find(([n]) => n === 1)?.[1];
  if (!x1) throw new Error("need the N=1 point");
  const rows = points.filter(([n]) => n > 1).map(([n, x]) => ({ a: n - 1, b: n * (n - 1), y: n / (x / x1) - 1 }));
  let saa = 0, sab = 0, sbb = 0, say = 0, sby = 0;
  for (const r of rows) {
    saa += r.a * r.a; sab += r.a * r.b; sbb += r.b * r.b; say += r.a * r.y; sby += r.b * r.y;
  }
  const det = saa * sbb - sab * sab;
  const sigma = (say * sbb - sby * sab) / det;
  const kappa = (saa * sby - sab * say) / det;
  return { sigma, kappa, nPeak: uslPeakN(sigma, kappa), xPeak: x1 * usl(uslPeakN(sigma, kappa), sigma, kappa) };
}

export function examples() {
  const bun = { lambda: 4000, serviceMs: 0.2, k: 128 };
  const bunMu = 1000 / bun.serviceMs;
  const tokio = { lambda: 3200, serviceMs: 2, c: 8 };
  const tokioMu = 1000 / tokio.serviceMs;
  return {
    little: { lambda: 4000, wSeconds: 0.005, l: little(4000, 0.005) },
    amdahl: [0.5, 0.9, 0.95, 0.99].map((p) => ({ p, s8: amdahl(p, 8), s64: amdahl(p, 64), limit: 1 / (1 - p) })),
    gustafson: { alpha: 0.05, n: 64, s: gustafson(0.05, 64), amdahlSameSerialAtFixedSize: amdahl(0.95, 64) },
    usl: { sigma: 0.05, kappa: 0.001, nPeak: uslPeakN(0.05, 0.001), xPeak: usl(uslPeakN(0.05, 0.001), 0.05, 0.001), x8: usl(8, 0.05, 0.001), x64: usl(64, 0.05, 0.001) },
    uslFit: fitUsl([[1, 1000], [2, 1900], [4, 3480], [8, 5900], [16, 8400], [32, 8900], [64, 5800]]),
    bunServe: { ...bun, mu: bunMu, ...mm1k(bun.lambda, bunMu, bun.k), overload: mm1k(6000, bunMu, bun.k) },
    tokioServe: { ...tokio, mu: tokioMu, ...mmc(tokio.lambda, tokioMu, tokio.c), tail10ms: mmcWaitTail(tokio.lambda, tokioMu, tokio.c, 0.01), erlangB: erlangB(tokio.c, tokio.lambda / tokioMu) },
    bunMeasured: (() => { const s = 1 / 3586; const lambda = 0.8 / s; return { mu: 3586, sMs: s * 1000, lambda, md1: mg1(lambda, s, 0), mm1: mg1(lambda, s, 1) }; })(),
    mm1Contrast: { lambda: 400, mu: 500, rho: 0.8, wMs: 1000 / (500 - 400) },
  };
}

if (import.meta.main) {
  const [cmd, file] = Bun.argv.slice(2);
  if (cmd === "fit" && file) {
    const pts = (await Bun.file(file).text()).split("\n").filter(Boolean).map((l) => l.trim().split(/\s+/).map(Number) as [number, number]);
    console.log(JSON.stringify(fitUsl(pts), null, 2));
  } else console.log(JSON.stringify(examples(), null, 2));
}
