//! 数据库迁移

use crate::error::CoreResult;
use refinery::embed_migrations;

embed_migrations!("migrations");

/// 执行迁移
pub fn run(conn: &mut rusqlite::Connection) -> CoreResult<()> {
    let report = migrations::runner().run(conn)?;
    tracing::info!("迁移完成，应用 {} 个迁移", report.applied_migrations().len());
    Ok(())
}
