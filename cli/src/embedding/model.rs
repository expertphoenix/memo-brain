use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Embedding 模型客户端 - 支持 OpenAI 兼容 API
pub struct EmbeddingModel {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
    #[allow(dead_code)]
    dimension: usize,
    provider: ProviderType,
}

/// 提供商类型
#[derive(Debug, Clone)]
enum ProviderType {
    ZhipuAI, // 智谱 AI（默认）
    OpenAI,
    Ollama,
}

impl EmbeddingModel {
    /// 创建新的 embedding 客户端
    ///
    /// # 参数
    /// - `api_key`: API 密钥
    /// - `model`: 模型名称
    /// - `base_url`: API 端点
    /// - `dimension`: embedding 维度(可选,自动推断)
    /// - `provider`: 提供商类型(可选: "openai", "ollama")
    pub fn new(
        api_key: String,
        model: String,
        base_url: Option<String>,
        dimension: Option<usize>,
        provider: Option<String>,
    ) -> Result<Self> {
        // 推断提供商和 base_url
        let (provider, base_url) = Self::infer_provider(&base_url, &provider);

        let client = Client::new();
        let dimension = dimension.unwrap_or_else(|| Self::infer_dimension(&model));

        Ok(Self {
            client,
            api_key,
            model,
            base_url,
            dimension,
            provider,
        })
    }

    /// 推断提供商类型
    fn infer_provider(
        base_url: &Option<String>,
        provider: &Option<String>,
    ) -> (ProviderType, String) {
        // 优先使用配置中指定的 provider
        if let Some(p) = provider {
            let provider_type = match p.to_lowercase().as_str() {
                "zhipu" | "zhipuai" | "bigmodel" => ProviderType::ZhipuAI,
                "ollama" => ProviderType::Ollama,
                "openai" => ProviderType::OpenAI,
                _ => {
                    tracing::warn!("Unknown provider '{}', defaulting to ZhipuAI", p);
                    ProviderType::ZhipuAI
                }
            };

            let url = base_url.clone().unwrap_or_else(|| match provider_type {
                ProviderType::ZhipuAI => "https://open.bigmodel.cn/api/paas/v4".to_string(),
                ProviderType::Ollama => "http://localhost:11434/api".to_string(),
                ProviderType::OpenAI => "https://api.openai.com/v1".to_string(),
            });

            tracing::debug!("Using provider: {:?}, base_url: {}", provider_type, url);
            return (provider_type, url);
        }

        // 根据 base_url 自动推断
        let result = match base_url {
            Some(url) => {
                if url.contains("localhost") || url.contains("127.0.0.1") || url.contains("ollama")
                {
                    (ProviderType::Ollama, url.clone())
                } else if url.contains("bigmodel.cn") || url.contains("zhipu") {
                    (ProviderType::ZhipuAI, url.clone())
                } else if url.contains("openai.com") {
                    (ProviderType::OpenAI, url.clone())
                } else {
                    // 其他 OpenAI 兼容接口
                    (ProviderType::OpenAI, url.clone())
                }
            }
            None => (
                ProviderType::ZhipuAI,
                "https://open.bigmodel.cn/api/paas/v4".to_string(),
            ),
        };

        tracing::debug!("Inferred provider: {:?}, base_url: {}", result.0, result.1);
        result
    }

    /// 根据模型名称推断维度
    fn infer_dimension(model: &str) -> usize {
        let dimension =
        // 智谱 AI 模型
        if model == "embedding-3" {
            2048 // 默认 2048，支持 256/512/1024/2048
        } else if model == "embedding-2" {
            1024 // 固定 1024
        }
        // OpenAI 模型
        else if model.contains("text-embedding-3-large") {
            3072
        } else if model.contains("text-embedding-3-small") || model.contains("text-embedding-ada") {
            1536
        }
        // Ollama 模型
        else if model.contains("nomic") {
            768
        }
        // Jina 模型
        else if model.contains("jina") && model.contains("v3") {
            1024
        }
        // 默认维度（智谱 AI embedding-3）
        else {
            2048
        };

        tracing::debug!("Inferred dimension {} for model '{}'", dimension, model);
        dimension
    }

    /// 获取 embedding 维度
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// 对单个文本生成 embedding
    pub async fn encode(&self, text: &str) -> Result<Vec<f32>> {
        match self.provider {
            ProviderType::Ollama => self.encode_ollama(text).await,
            ProviderType::ZhipuAI | ProviderType::OpenAI => {
                self.encode_openai_compatible(text).await
            }
        }
    }

    /// OpenAI 兼容格式(OpenAI、Jina、Azure 等)
    async fn encode_openai_compatible(&self, text: &str) -> Result<Vec<f32>> {
        #[derive(Serialize)]
        struct Request {
            input: String,
            model: String,
        }

        #[derive(Deserialize)]
        struct Response {
            data: Vec<EmbeddingData>,
        }

        #[derive(Deserialize)]
        struct EmbeddingData {
            embedding: Vec<f32>,
        }

        let url = format!("{}/embeddings", self.base_url);
        let request = Request {
            input: text.to_string(),
            model: self.model.clone(),
        };

        let mut req = self.client.post(&url).json(&request);

        // 添加认证头
        if !self.api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.api_key));
        }

        let response = req
            .send()
            .await
            .context("Failed to send embedding request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Embedding API error ({}): {}", status, error_text);
        }

        let api_response: Response = response
            .json()
            .await
            .context("Failed to parse embedding response")?;

        api_response
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .context("No embedding returned")
    }

    /// Ollama 格式
    async fn encode_ollama(&self, text: &str) -> Result<Vec<f32>> {
        #[derive(Serialize)]
        struct Request {
            model: String,
            input: String,
        }

        #[derive(Deserialize)]
        struct Response {
            embeddings: Vec<Vec<f32>>,
        }

        let url = format!("{}/embed", self.base_url);
        let request = Request {
            model: self.model.clone(),
            input: text.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send Ollama embedding request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Ollama API error ({}): {}", status, error_text);
        }

        let api_response: Response = response
            .json()
            .await
            .context("Failed to parse Ollama response")?;

        api_response
            .embeddings
            .into_iter()
            .next()
            .context("No embedding returned from Ollama")
    }

    /// 对多个文本批量生成 embeddings
    #[allow(dead_code)]
    pub async fn encode_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.encode(&text).await?);
        }
        Ok(results)
    }
}
