//! drive_cdn (stub)

use worker::{Context, Env, Request, Response, Result};

pub async fn handle(_req: Request, _env: Env, _ctx: Context) -> Result<Response> {
    Response::error("not ported", 501)
}
