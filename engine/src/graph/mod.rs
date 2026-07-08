mod node;
mod edge;
#[allow(clippy::module_inception)]
mod graph;
pub mod merge;
pub mod resolver;
pub mod query;

pub use node::{Node, NodeKind};
pub use edge::{Edge, EdgeKind};
pub use graph::Graph;
pub use merge::GraphMerger;
pub use resolver::CrossFileResolver;
