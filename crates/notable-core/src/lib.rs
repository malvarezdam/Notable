//! `notable-core`: modelo de datos (árbol de nodos), motor de cálculo y nota mínima.
//!
//! Rust puro, sin interfaz ni base de datos. Ver `PLAN.md`.

mod calc;
mod config;
mod error;
mod id;
mod node;
mod tree;

pub use calc::{Evaluation, NodeResult, Warning};
pub use config::{Aggregation, Conversion, GradeConfig, Rounding};
pub use error::{ConfigError, TreeError};
pub use id::NodeId;
pub use node::Node;
pub use tree::Tree;
