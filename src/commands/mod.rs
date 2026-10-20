pub mod config;
pub mod role;
pub mod say;
pub mod session;
pub mod setup;
pub mod status;
pub mod task;
#[cfg(test)]
pub mod testing;

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

#[cfg(test)]
mod tests {
    use super::*;
    use testing::{crew_ctx, ctx_in, scratch_dir};

    #[test]
    fn watch_needs_a_crew() {
        let ctx = ctx_in(&scratch_dir(), None);
        assert!(watch(&ctx, true).unwrap_err().to_string().contains("crew setup"));
    }

    #[test]
    fn watch_once_records_what_each_agent_screen_looks_like() {
        let ctx = crew_ctx();
        watch(&ctx, true).unwrap();
        for role in ["lead", "builder"] {
            let seen = std::fs::read_to_string(ctx.state.dir().join(format!("watch-{role}"))).unwrap();
            assert!(seen.trim_end().ends_with(" no"), "{role}: {seen}");
        }
    }
}
