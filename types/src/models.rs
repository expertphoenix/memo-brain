use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 核心记忆数据结构（完全独立，不依赖任何数据库）
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

/// 查询结果（用于返回搜索/列表结果）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub id: String,
    pub content: String,
    pub tags: Vec<String>,
    pub updated_at: i64,
    pub score: Option<f32>,
}

/// 时间范围过滤
#[derive(Debug, Clone)]
pub struct TimeRange {
    pub after: Option<i64>,
    pub before: Option<i64>,
}

/// 用于构建 Memory 的 Builder
pub struct MemoryBuilder {
    pub content: String,
    pub tags: Vec<String>,
    pub vector: Vec<f32>,
    pub source_file: Option<String>,
}

impl Memory {
    pub fn new(builder: MemoryBuilder) -> Self {
        use uuid::Uuid;
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

/// Markdown 解析相关（非数据库）
#[derive(Debug, Clone)]
pub struct MemoSection {
    pub content: String,
    pub metadata: MemoMetadata,
}

#[derive(Debug, Clone)]
pub struct MemoMetadata {
    pub tags: Vec<String>,
}

/// 记忆树（树状搜索结果）
#[derive(Debug, Clone)]
pub struct MemoryTree {
    pub root: MemoryNode,
    pub total_nodes: usize,
}

/// 树节点
#[derive(Debug, Clone)]
pub struct MemoryNode {
    pub memory: Option<QueryResult>, // None for root node
    pub children: Vec<MemoryNode>,
    pub layer: usize,
}

/// 树搜索配置
#[derive(Debug, Clone)]
pub struct TreeSearchConfig {
    pub first_threshold: f32,      // 第一层阈值
    pub max_depth: usize,          // 硬性上限（默认5）
    pub max_nodes: usize,          // 最大节点数（防爆炸，默认100）
    pub branch_limit: usize,       // 每层分支数（默认5）
    pub require_tag_overlap: bool, // 第二层+是否要求标签重叠
}

impl Default for TreeSearchConfig {
    fn default() -> Self {
        Self {
            first_threshold: 0.60,
            max_depth: 5,
            max_nodes: 100,
            branch_limit: 5,
            require_tag_overlap: true,
        }
    }
}

impl TreeSearchConfig {
    /// 创建树搜索配置（简化接口）
    ///
    /// # 参数
    /// - `first_threshold`: 第一层搜索阈值
    /// - `max_nodes`: 最大节点数
    pub fn new(first_threshold: f32, max_nodes: usize) -> Self {
        Self {
            first_threshold,
            max_nodes,
            ..Default::default()
        }
    }

    /// 根据起始阈值计算各层阈值
    pub fn generate_thresholds(&self) -> Vec<f32> {
        let mut thresholds = vec![self.first_threshold];
        let mut current = self.first_threshold;

        while thresholds.len() < self.max_depth && current < 0.95 {
            let increment = self.calculate_increment(current);
            let next = current + increment;

            if next > 0.95 {
                if 0.95 - current >= 0.03 {
                    thresholds.push(0.95);
                }
                break;
            }

            thresholds.push(next);
            current = next;
        }

        thresholds
    }

    /// 自适应增量计算
    fn calculate_increment(&self, current_threshold: f32) -> f32 {
        if current_threshold < 0.65 {
            0.10 // 有足够空间
        } else if current_threshold < 0.75 {
            0.07 // 中等空间
        } else if current_threshold < 0.85 {
            0.05 // 空间紧张
        } else {
            0.03 // 空间很小
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_threshold_generation() {
        let config = TreeSearchConfig::default();
        let thresholds = config.generate_thresholds();

        // 默认起始 0.60，应该生成多层递增的阈值
        assert!(thresholds.len() >= 3, "Should generate at least 3 layers");
        assert!(thresholds.len() <= 5, "Should not exceed max_depth");

        // 第一层应该是起始值
        assert_eq!(thresholds[0], 0.60);

        // 阈值应该递增
        for i in 1..thresholds.len() {
            assert!(
                thresholds[i] > thresholds[i - 1],
                "Thresholds should be increasing"
            );
        }

        // 最后一层不应超过 0.95
        assert!(thresholds[thresholds.len() - 1] <= 0.95);
    }

    #[test]
    fn test_high_start_threshold() {
        let config = TreeSearchConfig::new(0.70, 100);
        let thresholds = config.generate_thresholds();

        // 起始 0.70，应该生成多层
        assert!(thresholds.len() >= 2);
        assert_eq!(thresholds[0], 0.70);

        // 阈值应该递增
        for i in 1..thresholds.len() {
            assert!(thresholds[i] > thresholds[i - 1]);
        }

        // 增量应该逐渐变小（因为空间变小）
        if thresholds.len() >= 3 {
            let inc1 = thresholds[1] - thresholds[0];
            let inc2 = thresholds[2] - thresholds[1];
            assert!(
                inc1 >= inc2 || (inc1 - inc2).abs() < 0.001,
                "Increments should decrease or stay similar as threshold increases"
            );
        }
    }

    #[test]
    fn test_very_high_start_threshold() {
        let config = TreeSearchConfig::new(0.85, 100);
        let thresholds = config.generate_thresholds();

        // 起始 0.85，空间很小，增量 0.03 或 0.05
        assert!(thresholds.len() >= 2);
        assert_eq!(thresholds[0], 0.85);
        // 第二层应该在 0.90 或 0.95
        assert!(thresholds[1] >= 0.88 && thresholds[1] <= 0.95);
    }

    #[test]
    fn test_near_limit_threshold() {
        let config = TreeSearchConfig::new(0.93, 100);
        let thresholds = config.generate_thresholds();

        // 起始 0.93，只能生成 1-2 层
        assert!(thresholds.len() <= 2);
        assert_eq!(thresholds[0], 0.93);
    }
}
