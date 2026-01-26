use anyhow::Result;

use crate::config::Config;
use crate::db::{Connection, TableOperations};
use crate::ui::Output;

/// 清空数据库（高危操作）
pub async fn clear(local: bool, global: bool, skip_confirm: bool) -> Result<()> {
    let output = Output::new();

    // 验证作用域标志
    Config::validate_scope_flags(local, global)?;

    // 确定要清空的 brain 目录路径
    let scope_name = Config::get_scope_name(local, global);
    let brain_path = if local {
        Config::local_memo_dir().join("brain")
    } else if global {
        Config::global_memo_dir().join("brain")
    } else {
        Config::load()?.brain_path
    };

    // 检查数据库是否存在
    if !brain_path.exists() {
        output.database_info(&brain_path, 0);
        output.info("Database is empty, nothing to clear.");
        return Ok(());
    }

    // 尝试获取记录数
    let record_count = if let Ok(conn) = Connection::connect(&brain_path).await {
        if TableOperations::table_exists(conn.inner(), "memories").await {
            if let Ok(table) = TableOperations::open_table(conn.inner(), "memories").await {
                table.count_rows(None).await.unwrap_or(0)
            } else {
                0
            }
        } else {
            0
        }
    } else {
        0
    };

    // 显示数据库信息
    output.database_info(&brain_path, record_count);

    // 显示警告信息
    output.warning("this will delete all memories");
    output.info(&format!(
        "{} database: {}",
        scope_name,
        brain_path.display()
    ));
    output.info(&format!("{} records will be deleted", record_count));

    // 确认操作
    if !skip_confirm && !output.confirm("yes")? {
        output.info("Operation cancelled");
        return Ok(());
    }

    // 执行清空操作
    output.begin_operation("Clearing", "database");

    // 删除整个 brain 目录
    if brain_path.exists() {
        std::fs::remove_dir_all(&brain_path)?;
    }

    // 重新创建空的数据库
    std::fs::create_dir_all(&brain_path)?;
    let conn = Connection::connect(&brain_path).await?;
    TableOperations::create_table(conn.inner(), "memories").await?;

    output.finish("clearing", scope_name);

    Ok(())
}
