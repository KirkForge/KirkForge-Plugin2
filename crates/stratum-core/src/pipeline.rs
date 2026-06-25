use crate::content::ContentType;
use crate::store::OffloadStore;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineConfig {
    pub reformat_target_ratio: f32,
    pub bloat_threshold: f32,
    pub offload_fallback_ratio: f32,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            reformat_target_ratio: 0.05,
            bloat_threshold: 0.5,
            offload_fallback_ratio: 0.85,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CompressionContext {
    pub query: Option<String>,
    pub token_budget: Option<usize>,
}

pub struct CompressionPipeline;

impl CompressionPipeline {
    pub fn new() -> Self {
        Self
    }

    pub fn run(
        &self,
        content: &str,
        _content_type: ContentType,
        _ctx: &CompressionContext,
        _store: &dyn OffloadStore,
    ) -> String {
        // MVP stub: return the input unchanged. The orchestrator described in
        // ADR-0005 will be wired here once transforms are implemented.
        content.to_string()
    }
}

impl Default for CompressionPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryOffloadStore;

    #[test]
    fn pipeline_stub_returns_input() {
        let pipeline = CompressionPipeline::new();
        let store = InMemoryOffloadStore::new();
        let input = "some agent context";
        let out = pipeline.run(input, ContentType::PlainText, &CompressionContext::default(), &store);
        assert_eq!(out, input);
    }
}
