use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use std::collections::HashSet;

use crate::config::Config;
use crate::embedding::EmbeddingModel;
use crate::rerank::RerankModel;
use crate::ui::Output;
use memo_local::LocalStorageClient;
use memo_types::{
    SearchConfig as MultiLayerSearchConfig, StorageBackend, StorageConfig, TimeRange,
};

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

    let _initialized = crate::service::init::ensure_initialized().await?;
    let config = Config::load_with_scope(force_local, force_global)?;
    config.validate_api_key(force_local)?;

    let model = EmbeddingModel::new(
        config.embedding_api_key.clone(),
        config.embedding_model.clone(),
        config.embedding_base_url.clone(),
        config.embedding_dimension,
        config.embedding_provider.clone(),
    )?;

    let storage_config = StorageConfig {
        path: config.brain_path.to_string_lossy().to_string(),
        dimension: model.dimension(),
    };
    let storage = LocalStorageClient::connect(&storage_config).await?;
    let record_count = storage.count().await?;

    output.database_info(&config.brain_path, record_count);
    output.status("Encoding", "query");

    let query_vector = model.encode(&query).await?;

    multi_layer_search(MultiLayerSearchParams {
        query_vector,
        query: &query,
        limit,
        threshold,
        after,
        before,
        storage: &storage,
        config: &config,
        output: &output,
    })
    .await
}

struct MultiLayerSearchParams<'a> {
    query_vector: Vec<f32>,
    query: &'a str,
    limit: usize,
    threshold: f32,
    after: Option<String>,
    before: Option<String>,
    storage: &'a LocalStorageClient,
    config: &'a Config,
    output: &'a Output,
}

/// Multi-layer search with reranking
async fn multi_layer_search(params: MultiLayerSearchParams<'_>) -> Result<()> {
    let MultiLayerSearchParams {
        query_vector,
        query,
        limit,
        threshold,
        after,
        before,
        storage,
        config,
        output,
    } = params;

    let max_nodes = if limit < 10 { 50 } else { limit * 10 };
    let search_config = MultiLayerSearchConfig::new(threshold, max_nodes);
    let thresholds = search_config.generate_thresholds();
    let max_layers = thresholds.len().min(search_config.max_depth);

    tracing::debug!(
        "Multi-layer search: max_nodes={}, layers={}, thresholds={:?}",
        max_nodes,
        max_layers,
        thresholds
    );

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

    let mut visited = HashSet::new();
    let mut all_candidates = Vec::new();

    output.status("Searching", "layer 1");
    let mut current_layer_results = storage
        .search_by_vector(
            query_vector,
            search_config.branch_limit,
            thresholds[0],
            time_range.clone(),
        )
        .await?;

    if current_layer_results.is_empty() {
        output.info(&format!(
            "No results found above threshold {:.2}",
            thresholds[0]
        ));
        output.note("Try lowering the threshold with -t/--threshold option");
        return Ok(());
    }

    tracing::debug!("Layer 1: found {} results", current_layer_results.len());

    for result in &current_layer_results {
        if visited.insert(result.id.clone()) {
            all_candidates.push(result.clone());
        }
    }

    for (layer_index, &layer_threshold) in
        thresholds.iter().enumerate().skip(1).take(max_layers - 1)
    {
        if all_candidates.len() >= max_nodes {
            tracing::debug!("Reached max_nodes limit: {}", max_nodes);
            break;
        }

        if current_layer_results.is_empty() {
            break;
        }

        output.status("Searching", &format!("layer {}", layer_index + 1));

        let mut next_layer_results = Vec::new();

        for result in &current_layer_results {
            let memory = match storage.find_memory_by_id(&result.id).await? {
                Some(m) => m,
                None => continue,
            };

            let mut related = storage
                .search_by_vector(
                    memory.vector.clone(),
                    search_config.branch_limit * 2,
                    layer_threshold,
                    time_range.clone(),
                )
                .await?;

            if layer_index >= 1 && search_config.require_tag_overlap {
                related.retain(|r| r.tags.iter().any(|t| memory.tags.contains(t)));
            }

            for rel in related.into_iter().take(search_config.branch_limit) {
                if visited.insert(rel.id.clone()) {
                    all_candidates.push(rel.clone());
                    next_layer_results.push(rel);

                    if all_candidates.len() >= max_nodes {
                        break;
                    }
                }
            }

            if all_candidates.len() >= max_nodes {
                break;
            }
        }

        tracing::debug!(
            "Layer {}: found {} new results, total candidates: {}",
            layer_index + 1,
            next_layer_results.len(),
            all_candidates.len()
        );

        current_layer_results = next_layer_results;
    }

    tracing::debug!(
        "Multi-layer search completed: {} unique candidates",
        all_candidates.len()
    );

    output.status("Reranking", &format!("{} candidates", all_candidates.len()));

    let rerank_model = RerankModel::new(
        config.rerank_api_key.clone(),
        config.rerank_model.clone(),
        config.rerank_base_url.clone(),
    )?;

    let documents: Vec<&str> = all_candidates.iter().map(|r| r.content.as_str()).collect();
    let reranked = rerank_model.rerank(query, &documents, Some(limit)).await?;

    tracing::debug!("Rerank returned {} results", reranked.len());

    let mut final_results = Vec::new();
    for item in &reranked {
        if let Some(result) = all_candidates.get(item.index) {
            let mut reranked_result = result.clone();
            reranked_result.score = Some(item.score as f32);
            final_results.push(reranked_result);

            tracing::debug!(
                "Reranked: index={}, score={:.4}, id={}",
                item.index,
                item.score,
                result.id
            );
        }
    }

    output.search_results(&final_results);
    Ok(())
}

fn parse_datetime(input: &str) -> Result<i64> {
    if let Ok(dt) = NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M") {
        return Ok(dt.and_utc().timestamp_millis());
    }

    if let Ok(date) = chrono::NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        let dt = date
            .and_hms_opt(0, 0, 0)
            .context("Failed to create datetime")?;
        return Ok(dt.and_utc().timestamp_millis());
    }

    anyhow::bail!("Invalid date format. Use YYYY-MM-DD or YYYY-MM-DD HH:MM")
}
