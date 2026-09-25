pub mod detector;
pub mod limits;
pub mod processor;
pub mod resolver;
pub mod types;

pub use detector::detect_from_path;
pub use resolver::resolve_strategy;
pub use types::*;
