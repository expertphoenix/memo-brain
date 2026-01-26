use anyhow::{Context, Result};
use arrow_array::{
    ArrayRef, RecordBatch, RecordBatchIterator, StringArray, TimestampMillisecondArray,
};
use chrono::Utc;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use std::sync::Arc;
use uuid::Uuid;

use crate::config::Config;
use crate::db::{Connection, TableOperations};
use crate::embedding::EmbeddingModel;
use crate::ui::Output;

pub async fn merge(
    ids: Vec<String>,
    content: String,
    tags: Option<Vec<String>>,
    force_local: bool,
    force_global: bool,
) -> Result<()> {
    let output = Output::new();
    let config = Config::load_with_scope(force_local, force_global)?;
    let scope = Config::get_scope_name(force_local, force_global);

    if ids.len() < 2 {
        anyhow::bail!("Need at least 2 memory IDs to merge");
    }

    let conn = Connection::connect(&config.brain_path).await?;
    let table = TableOperations::open_table(conn.inner(), "memories").await?;
    let record_count = table.count_rows(None).await.unwrap_or(0);

    output.database_info(&config.brain_path, record_count);

    // 检查 API key
    config.validate_api_key(force_local)?;

    // 验证所有记忆是否存在，并收集信息
    output.status("Collecting", &format!("{} memories", ids.len()));

    let mut oldest_created_at = i64::MAX;
    let mut merged_tags: Vec<String> = Vec::new();

    for id in &ids {
        let mut stream = table
            .query()
            .only_if(format!("id = '{}'", id))
            .select(lancedb::query::Select::columns(&["tags", "created_at"]))
            .execute()
            .await?;

        let batch = stream
            .try_next()
            .await?
            .context(format!("Memory not found with ID: {}", id))?;

        if batch.num_rows() == 0 {
            anyhow::bail!("Memory not found with ID: {}", id);
        }

        // 获取 tags
        let tags_col = batch
            .column_by_name("tags")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .context("Failed to get tags column")?;
        let memory_tags: Vec<String> = serde_json::from_str(tags_col.value(0))?;

        // 合并 tags（去重）
        for tag in memory_tags {
            if !merged_tags.contains(&tag) {
                merged_tags.push(tag);
            }
        }

        // 获取最早的 created_at
        let created_ats = batch
            .column_by_name("created_at")
            .and_then(|c| c.as_any().downcast_ref::<TimestampMillisecondArray>())
            .context("Failed to get created_at column")?;
        let created_at = created_ats.value(0);
        if created_at < oldest_created_at {
            oldest_created_at = created_at;
        }
    }

    // 使用用户提供的 tags 或合并后的 tags
    let final_tags = tags.unwrap_or(merged_tags);

    // 生成新的嵌入向量
    let model = EmbeddingModel::new(
        config.embedding_api_key.clone(),
        config.embedding_model.clone(),
        config.embedding_base_url.clone(),
        config.embedding_dimension,
        config.embedding_provider.clone(),
    )?;

    output.status("Encoding", "merged content");
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let embedding = model.encode(&normalized).await?;

    // 插入合并后的新记忆
    output.status("Merging", &format!("{} memories", ids.len()));
    let new_id = Uuid::new_v4().to_string();
    let updated_at_ms = Utc::now().timestamp_millis();

    let id_array = StringArray::from(vec![new_id.as_str()]);
    let content_array = StringArray::from(vec![content.as_str()]);
    let tags_json = serde_json::to_string(&final_tags)?;
    let tags_array = StringArray::from(vec![tags_json.as_str()]);
    let source_file_array = StringArray::from(vec![""]);

    let vector_values: Vec<Option<f32>> = embedding.iter().map(|&v| Some(v)).collect();
    let vector_array = arrow_array::FixedSizeListArray::from_iter_primitive::<
        arrow_array::types::Float32Type,
        _,
        _,
    >(vec![Some(vector_values)], model.dimension() as i32);

    let created_at_array = TimestampMillisecondArray::from(vec![Some(oldest_created_at)]);
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

    // 删除旧记忆
    for id in &ids {
        table.delete(&format!("id = '{}'", id)).await?;
    }

    // 插入新记忆
    let schema = crate::models::memory_schema();
    let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
    table.add(Box::new(batches)).execute().await?;

    output.finish("merge", scope);

    Ok(())
}
