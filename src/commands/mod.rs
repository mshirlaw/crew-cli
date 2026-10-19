pub mod config;
pub mod role;
pub mod say;
pub mod session;
pub mod setup;
pub mod status;
pub mod task;

use crate::crew::context::Ctx;
use crate::crew::watcher;
use anyhow::Result;

pub fn watch(ctx: &Ctx, once: bool) -> Result<()> {
    ctx.require_crew()?;
    if once {
        watcher::check_all(ctx)
    } else {
        watcher::run(ctx);
        Ok(())
    }
}
