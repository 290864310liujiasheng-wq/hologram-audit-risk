mod edge;
#[allow(clippy::module_inception)]
mod graph;
pub mod merge;
mod node;
pub mod query;
pub mod resolver;

pub use edge::{Edge, EdgeKind};
pub use graph::Graph;
pub use merge::GraphMerger;
pub use node::{Node, NodeKind};
pub use resolver::CrossFileResolver;
