//! `notable-core`: modelo de datos (árbol de nodos), motor de cálculo y nota mínima.
//!
//! Rust puro, sin interfaz ni base de datos. Ver `PLAN.md`.

mod calc;
mod config;
mod error;
mod goal;
mod id;
mod node;
mod rules;
mod tree;

pub use calc::{Evaluation, NodeResult, Warning};
pub use config::{Aggregation, Conversion, GradeConfig, Rounding};
pub use error::{ConfigError, TreeError};
pub use goal::{MissingGrade, Requirement};
pub use id::NodeId;
pub use node::Node;
pub use rules::{Effect, Exemption, Minimum, ReplaceTarget, Replacement, Rules};
pub use tree::Tree;
