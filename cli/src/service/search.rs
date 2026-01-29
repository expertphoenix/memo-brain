use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use std::collections::HashSet;

use crate::config::Config;
use crate::embedding::EmbeddingModel;
use crate::ui::Output;
use memo_local::LocalStorageClient;
use memo_types::{
    Memory, MemoryNode, MemoryTree, QueryResult, StorageBackend, StorageConfig, TimeRange,
    TreeSearchConfig,
};

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

    // 使用树搜索模式（带时间过滤）
    search_as_tree(query_vector, limit, threshold, after, before, &storage, &output).await
}

/// 树状搜索
async fn search_as_tree(
    query_vector: Vec<f32>,
    limit: usize,
    threshold: f32,
    after: Option<String>,
    before: Option<String>,
    storage: &LocalStorageClient,
    output: &Output,
) -> Result<()> {
    output.status("Searching", "tree (layer 1)");

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

    // 构建树配置
    let max_nodes = if limit < 10 { 50 } else { limit * 10 };
    let config = TreeSearchConfig::new(threshold, max_nodes);

    // 生成各层阈值
    let thresholds = config.generate_thresholds();

    // 第一层搜索（应用时间过滤）
    let first_results = storage
        .search_by_vector(query_vector, config.branch_limit, thresholds[0], time_range.clone())
        .await?;

    if first_results.is_empty() {
        output.info(&format!(
            "No results found above threshold {:.2}",
            thresholds[0]
        ));
        return Ok(());
    }

    // 构建树
    let mut visited = HashSet::new();
    let mut total_nodes = 0;

    let mut root = MemoryNode {
        memory: None,
        children: Vec::new(),
        layer: 0,
    };

    for result in first_results {
        visited.insert(result.id.clone());
        total_nodes += 1;

        let node = expand_node(
            result,
            1,
            &thresholds,
            &config,
            &time_range,
            storage,
            &mut visited,
            &mut total_nodes,
        )
        .await?;

        root.children.push(node);
    }

    let tree = MemoryTree { root, total_nodes };

    // 输出树
    output.search_results_tree(&tree);

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

/// 递归展开节点
fn expand_node<'a>(
    result: QueryResult,
    layer: usize,
    thresholds: &'a [f32],
    config: &'a TreeSearchConfig,
    time_range: &'a Option<TimeRange>,
    storage: &'a LocalStorageClient,
    visited: &'a mut HashSet<String>,
    total_nodes: &'a mut usize,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<MemoryNode>> + 'a>> {
    Box::pin(async move {
        let mut node = MemoryNode {
            memory: Some(result.clone()),
            children: Vec::new(),
            layer,
        };

        // 终止条件
        if layer >= thresholds.len()
            || layer >= config.max_depth
            || *total_nodes >= config.max_nodes
        {
            return Ok(node);
        }

        // 获取完整 Memory（需要 vector 和 tags）
        let memory = match storage.find_memory_by_id(&result.id).await? {
            Some(m) => m,
            None => return Ok(node), // 找不到就不展开
        };

        // 搜索相关记忆（使用记忆的向量，应用时间过滤）
        let related =
            search_related_memories(&memory, layer, thresholds[layer], config, time_range, storage).await?;

        // 递归展开
        for child_result in related {
            if visited.contains(&child_result.id) || child_result.id == result.id {
                continue;
            }

            visited.insert(child_result.id.clone());
            *total_nodes += 1;

            if *total_nodes >= config.max_nodes {
                break;
            }

            let child = expand_node(
                child_result,
                layer + 1,
                thresholds,
                config,
                time_range,
                storage,
                visited,
                total_nodes,
            )
            .await?;

            node.children.push(child);
        }

        Ok(node)
    })
}

/// 搜索相关记忆（核心）
async fn search_related_memories(
    memory: &Memory,
    layer: usize,
    threshold: f32,
    config: &TreeSearchConfig,
    time_range: &Option<TimeRange>,
    storage: &LocalStorageClient,
) -> Result<Vec<QueryResult>> {
    // 使用记忆的向量搜索（应用时间过滤）
    let mut candidates = storage
        .search_by_vector(
            memory.vector.clone(),
            config.branch_limit * 2,
            threshold,
            time_range.clone(),
        )
        .await?;

    // 第二层开始：标签过滤
    if layer >= 1 && config.require_tag_overlap {
        candidates.retain(|r| {
            // 标签至少有一个重叠
            r.tags.iter().any(|t| memory.tags.contains(t))
        });
    }

    Ok(candidates.into_iter().take(config.branch_limit).collect())
}
