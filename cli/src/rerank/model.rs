use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Rerank 模型
pub struct RerankModel {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
}

#[derive(Debug, Serialize)]
struct RerankRequest {
    model: String,
    query: String,
    documents: Vec<String>,
    top_n: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct RerankResponse {
    results: Vec<RerankResult>,
}

#[derive(Debug, Deserialize)]
struct RerankResult {
    index: usize,
    relevance_score: f64,
}

/// Rerank 结果
#[derive(Debug, Clone)]
pub struct RerankItem {
    pub index: usize,
    pub score: f64,
}

impl RerankModel {
    /// 创建 Rerank 模型
    pub fn new(api_key: String, model: String, base_url: Option<String>) -> Result<Self> {
        // 设置默认 base_url（智谱 AI）
        let base_url =
            base_url.unwrap_or_else(|| "https://open.bigmodel.cn/api/paas/v4".to_string());

        // 创建 HTTP Client（带超时，rerank 可能需要更长时间）
        let client = Client::builder().timeout(Duration::from_secs(60)).build()?;

        tracing::debug!(
            "Created RerankModel: model={}, base_url={}",
            model,
            base_url
        );

        Ok(Self {
            client,
            api_key,
            model,
            base_url,
        })
    }

    /// Rerank 文档
    pub async fn rerank(
        &self,
        query: &str,
        documents: &[&str],
        top_n: Option<usize>,
    ) -> Result<Vec<RerankItem>> {
        tracing::debug!("Reranking {} documents, top_n={:?}", documents.len(), top_n);

        let request = RerankRequest {
            model: self.model.clone(),
            query: query.to_string(),
            documents: documents.iter().map(|s| s.to_string()).collect(),
            top_n,
        };

        let url = format!("{}/rerank", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await?;
            tracing::error!("Rerank API error ({}): {}", status, error_text);
            anyhow::bail!("Rerank API error ({}): {}", status, error_text);
        }

        let rerank_response: RerankResponse = response.json().await?;

        tracing::debug!(
            "Rerank API returned {} results",
            rerank_response.results.len()
        );

        Ok(rerank_response
            .results
            .into_iter()
            .map(|r| RerankItem {
                index: r.index,
                score: r.relevance_score,
            })
            .collect())
    }
}
