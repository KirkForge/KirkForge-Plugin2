pub mod rules;

use stratum_core::mode::Mode;

const CANONICAL: &str = include_str!("../../../docs/rules/CANONICAL.md");

pub fn build_rules(mode: Mode) -> String {
    rules::filter_by_mode(CANONICAL, mode)
}
