"use strict";
let t = 0; function g(n: number): number { t++; return n === 0 ? 0 : g(n - 1); }
console.log("tail call 1e7 ->", g(1e7), "calls", t);
let u = 0; function h(n: number): number { u++; return n === 0 ? 0 : 1 + h(n - 1); }
try { h(1e7); } catch { console.log("non-tail depth", u); }

export {}; // script entry point kept a module: top-level await, names local to the file
