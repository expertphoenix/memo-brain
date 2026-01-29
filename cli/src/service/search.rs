use anyhow::{Context, Result};
use chrono::NaiveDateTime;

use crate::config::Config;
use crate::embedding::EmbeddingModel;
use crate::rerank::RerankModel;
use crate::ui::Output;
use memo_local::LocalStorageClient;
use memo_types::{StorageBackend, StorageConfig, TimeRange};

/// Search options
pub struct SearchOptions {
    pub query: String,
    pub limit: usize,
    pub threshold: f32,
    pub after: Option<String>,
    pub before: Option<String>,
    pub force_local: bool,
    pub force_global: bool,
}

pub async fn search(options: SearchOptions) -> Result<()> {
    let SearchOptions {
        query,
        limit,
        threshold,
        after,
        before,
        force_local,
        force_global,
    } = options;
    let output = Output::new();

    // 自动初始化
    let _initialized = crate::service::init::ensure_initialized().await?;

    let config = Config::load_with_scope(force_local, force_global)?;

    // 检查 API key（Ollama 不需要）
    config.validate_api_key(force_local)?;

    // 创建 embedding 模型
    let model = EmbeddingModel::new(
        config.embedding_api_key.clone(),
        config.embedding_model.clone(),
        config.embedding_base_url.clone(),
        config.embedding_dimension,
        config.embedding_provider.clone(),
    )?;

    // 创建存储客户端
    let storage_config = StorageConfig {
        path: config.brain_path.to_string_lossy().to_string(),
        dimension: model.dimension(),
    };
    let storage = LocalStorageClient::connect(&storage_config).await?;
    let record_count = storage.count().await?;

    // 显示数据库信息
    output.database_info(&config.brain_path, record_count);

    output.status("Encoding", "query");

    // 生成查询向量
    let query_vector = model.encode(&query).await?;

    // 向量搜索
    output.status("Searching", "database");

    // 解析时间过滤参数
    let time_range = if after.is_some() || before.is_some() {
        let after_ts = after.as_ref().map(|s| parse_datetime(s)).transpose()?;
        let before_ts = before.as_ref().map(|s| parse_datetime(s)).transpose()?;
        Some(TimeRange {
            after: after_ts,
            before: before_ts,
        })
    } else {
        None
    };

    // 使用较大的候选集（用于 rerank）
    // 候选集取 100 或 limit*5 中的较大值，确保有足够的候选供 rerank
    let candidate_limit = 100_usize.max(limit * 5);
    tracing::debug!(
        "Searching with candidate_limit={}, threshold={}, final_limit={}",
        candidate_limit,
        threshold,
        limit
    );

    let mut results = storage
        .search_by_vector(query_vector, candidate_limit, threshold, time_range)
        .await?;

    if results.is_empty() {
        output.info(&format!(
            "No results found above threshold {:.2}",
            threshold
        ));
        output.note("Try lowering the threshold with -t/--threshold option");
        return Ok(());
    }

    tracing::debug!("Found {} candidates for reranking", results.len());

    // 使用 rerank 重排序（rerank 是必须的）
    output.status("Reranking", "results");

    let rerank_model = RerankModel::new(
        config.rerank_api_key.clone(),
        config.rerank_model.clone(),
        config.rerank_base_url.clone(),
    )?;

    // 准备文档列表（使用引用避免克隆）
    let documents: Vec<&str> = results.iter().map(|r| r.content.as_str()).collect();

    // 调用 rerank（返回 top limit 个结果）
    let reranked = rerank_model.rerank(&query, &documents, Some(limit)).await?;

    tracing::debug!("Rerank returned {} results", reranked.len());

    // 根据 rerank 结果重新排序
    let mut reordered_results = Vec::new();
    for item in &reranked {
        if let Some(result) = results.get(item.index) {
            let mut reranked_result = result.clone();
            // 使用 rerank 分数替代原始相似度分数
            reranked_result.score = Some(item.score as f32);
            reordered_results.push(reranked_result);

            tracing::debug!(
                "Reranked result: index={}, score={:.4}, id={}",
                item.index,
                item.score,
                result.id
            );
        }
    }
    results = reordered_results;

    // 显示结果
    output.search_results(&results);

    Ok(())
}

/// 解析日期时间字符串
/// 支持格式：
/// - YYYY-MM-DD
/// - YYYY-MM-DD HH:MM
fn parse_datetime(input: &str) -> Result<i64> {
    // 尝试解析 YYYY-MM-DD HH:MM 格式
    if let Ok(dt) = NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M") {
        return Ok(dt.and_utc().timestamp_millis());
    }

    // 尝试解析 YYYY-MM-DD 格式（默认为当天 00:00）
    if let Ok(date) = chrono::NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        let dt = date
            .and_hms_opt(0, 0, 0)
            .context("Failed to create datetime")?;
        return Ok(dt.and_utc().timestamp_millis());
    }

    anyhow::bail!("Invalid date format. Use YYYY-MM-DD or YYYY-MM-DD HH:MM")
}
