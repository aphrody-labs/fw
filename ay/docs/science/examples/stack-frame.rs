use std::hint::black_box;
#[inline(never)]
fn f(n: u64, prev: usize) -> u64 {
    let local = [n; 4];
    let addr = black_box(&local) as *const _ as usize;
    if n == 3 { println!("frame bytes between calls: {}", prev - addr); }
    if n >= 4 { return 0; }
    black_box(f(n + 1, addr)) + local[0]
}
fn main() { let x = [0u8;1]; f(0, black_box(&x) as *const _ as usize + 0); }
