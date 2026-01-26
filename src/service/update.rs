use anyhow::{Context, Result};
use arrow_array::{
    ArrayRef, RecordBatch, RecordBatchIterator, StringArray, TimestampMillisecondArray,
};
use chrono::Utc;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use std::sync::Arc;

use crate::config::Config;
use crate::db::{Connection, TableOperations};
use crate::embedding::EmbeddingModel;
use crate::ui::Output;

pub async fn update(
    id: &str,
    content: String,
    tags: Option<Vec<String>>,
    force_local: bool,
    force_global: bool,
) -> Result<()> {
    let output = Output::new();
    let config = Config::load_with_scope(force_local, force_global)?;
    let scope = Config::get_scope_name(force_local, force_global);

    let conn = Connection::connect(&config.brain_path).await?;
    let table = TableOperations::open_table(conn.inner(), "memories").await?;
    let record_count = table.count_rows(None).await.unwrap_or(0);

    output.database_info(&config.brain_path, record_count);

    // 检查 API key
    config.validate_api_key(force_local)?;

    // 查找要更新的记忆
    output.status("Finding", &format!("memory {}", id));

    let mut stream = table
        .query()
        .only_if(format!("id = '{}'", id))
        .execute()
        .await?;

    let batch = stream
        .try_next()
        .await?
        .context("Memory not found with given ID")?;

    if batch.num_rows() == 0 {
        anyhow::bail!("Memory not found with ID: {}", id);
    }

    // 获取原有记忆的信息
    let tags_col = batch
        .column_by_name("tags")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .context("Failed to get tags column")?;

    let source_files = batch
        .column_by_name("source_file")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .context("Failed to get source_file column")?;

    let created_ats = batch
        .column_by_name("created_at")
        .and_then(|c| c.as_any().downcast_ref::<TimestampMillisecondArray>())
        .context("Failed to get created_at column")?;

    let old_tags: Vec<String> = serde_json::from_str(tags_col.value(0))?;
    let source_file = if source_files.value(0).is_empty() {
        None
    } else {
        Some(source_files.value(0).to_string())
    };
    let created_at_ms = created_ats.value(0);

    // 使用新 tags 或保留原有 tags
    let final_tags = tags.unwrap_or(old_tags);

    // 生成新的嵌入向量
    let model = EmbeddingModel::new(
        config.embedding_api_key.clone(),
        config.embedding_model.clone(),
        config.embedding_base_url.clone(),
        config.embedding_dimension,
        config.embedding_provider.clone(),
    )?;

    output.status("Encoding", "new content");
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let embedding = model.encode(&normalized).await?;

    // 删除旧记忆并插入更新后的记忆
    output.status("Updating", &format!("memory {}", id));
    let updated_at_ms = Utc::now().timestamp_millis();

    let id_array = StringArray::from(vec![id]);
    let content_array = StringArray::from(vec![content.as_str()]);
    let tags_json = serde_json::to_string(&final_tags)?;
    let tags_array = StringArray::from(vec![tags_json.as_str()]);
    let source_file_array = StringArray::from(vec![source_file.as_deref().unwrap_or("")]);

    let vector_values: Vec<Option<f32>> = embedding.iter().map(|&v| Some(v)).collect();
    let vector_array = arrow_array::FixedSizeListArray::from_iter_primitive::<
        arrow_array::types::Float32Type,
        _,
        _,
    >(vec![Some(vector_values)], model.dimension() as i32);

    let created_at_array = TimestampMillisecondArray::from(vec![Some(created_at_ms)]);
    let updated_at_array = TimestampMillisecondArray::from(vec![Some(updated_at_ms)]);

    let batch = RecordBatch::try_new(
        crate::models::memory_schema(),
        vec![
            Arc::new(id_array) as ArrayRef,
            Arc::new(content_array) as ArrayRef,
            Arc::new(tags_array) as ArrayRef,
            Arc::new(vector_array) as ArrayRef,
            Arc::new(source_file_array) as ArrayRef,
            Arc::new(created_at_array) as ArrayRef,
            Arc::new(updated_at_array) as ArrayRef,
        ],
    )
    .context("Failed to create RecordBatch")?;

    // 先删除旧记忆
    table.delete(&format!("id = '{}'", id)).await?;

    // 插入更新后的记忆
    let schema = crate::models::memory_schema();
    let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
    table.add(Box::new(batches)).execute().await?;

    output.finish("update", scope);

    Ok(())
}
