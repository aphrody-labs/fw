use std::future::Future;
async fn yield_now() {}
async fn none() -> u8 { 1 }
async fn one_await() -> u8 { yield_now().await; 1 }
async fn held_1k() -> u8 { let a = [7u8; 1024]; yield_now().await; a[3] }
async fn held_then_drop() -> u8 { { let a = [7u8; 1024]; std::hint::black_box(&a); } yield_now().await; 1 }
async fn two_stage() -> u8 { let a = [7u8; 1024]; yield_now().await; let b = a[0]; let c = [9u8; 2048]; yield_now().await; c[1] + b }
async fn nested() -> u8 { held_1k().await }
async fn nested2() -> u8 { nested().await }
fn sz<F: Future>(_: &F) -> usize { std::mem::size_of::<F>() }
fn main() {
  println!("none {}", sz(&none()));
  println!("one_await {}", sz(&one_await()));
  println!("held_1k {}", sz(&held_1k()));
  println!("held_then_drop {}", sz(&held_then_drop()));
  println!("two_stage {}", sz(&two_stage()));
  println!("nested {}", sz(&nested()));
  println!("nested2 {}", sz(&nested2()));
}
