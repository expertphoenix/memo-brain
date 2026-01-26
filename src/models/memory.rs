use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub content: String,
    pub tags: Vec<String>,
    pub vector: Vec<f32>,
    pub source_file: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct MemoryBuilder {
    pub content: String,
    pub tags: Vec<String>,
    pub vector: Vec<f32>,
    pub source_file: Option<String>,
}

impl Memory {
    pub fn new(builder: MemoryBuilder) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            content: builder.content,
            tags: builder.tags,
            vector: builder.vector,
            source_file: builder.source_file,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoSection {
    pub content: String,
    pub metadata: MemoMetadata,
}

#[derive(Debug, Clone)]
pub struct MemoMetadata {
    pub tags: Vec<String>,
}
