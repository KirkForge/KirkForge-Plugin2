pub mod content;
pub mod error;
pub mod mode;
pub mod pipeline;
pub mod store;

pub mod config {
    use crate::pipeline::PipelineConfig;

    impl PipelineConfig {
        pub fn from_str(s: &str) -> Result<Self, toml::de::Error> {
            toml::from_str(s)
        }
    }
}
