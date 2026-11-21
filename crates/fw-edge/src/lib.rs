//! `fw-edge`: the one Cloudflare Worker for fw and aphrody, written in Rust on workers-rs.
//!
//! Routing order (the TypeScript worker's order, `cloudflare/worker.ts`):
//! `/drive*` is the Google Drive CDN, `/files*` is fw-drive on D1 + R2, everything else is the Edge OS
//! surface (`os`, `ai`, `proto`), including the `FwOSAgent` Durable Object.

use worker::{Context, Env, Request, Response, Result, event};

pub mod access;
pub mod ai;
pub mod drive_cdn;
pub mod os;
pub mod proto;

#[event(fetch)]
async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    let path = req.path();
    if path == "/drive" || path.starts_with("/drive/") {
        return drive_cdn::handle(req, env, ctx).await;
    }
    proto::route(req, env, ctx).await
}
