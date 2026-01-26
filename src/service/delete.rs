use anyhow::Result;

use crate::config::Config;
use crate::db::{Connection, TableOperations};
use crate::ui::Output;

pub async fn delete(
    id: &str,
    force_local: bool,
    force_global: bool,
    skip_confirm: bool,
) -> Result<()> {
    let output = Output::new();
    let config = Config::load_with_scope(force_local, force_global)?;
    let scope = Config::get_scope_name(force_local, force_global);

    let conn = Connection::connect(&config.brain_path).await?;
    let table = TableOperations::open_table(conn.inner(), "memories").await?;
    let record_count = table.count_rows(None).await.unwrap_or(0);

    output.database_info(&config.brain_path, record_count);

    // 显示警告信息
    output.warning(&format!("this will permanently delete memory {}", id));

    // 确认操作
    if !skip_confirm && !output.confirm("yes")? {
        output.info("Operation cancelled");
        return Ok(());
    }

    // 删除记忆
    output.begin_operation("Deleting", &format!("memory {}", id));
    table.delete(&format!("id = '{}'", id)).await?;

    output.finish("delete", scope);

    Ok(())
}
