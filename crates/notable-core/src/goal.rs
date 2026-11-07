//! Nota objetivo y nota mínima necesaria.
//!
//! Si un nodo tiene objetivo ([`crate::Node::target`]), se busca la menor nota que hace falta
//! en las evaluaciones que todavía no tienen nota. Si falta más de una, se supone la misma nota
//! en todas (en escalas distintas, el mismo lugar relativo de cada escala).
//!
//! La nota final de un nodo nunca baja al subir una nota que falta (promedios con pesos no
//! negativos, redondeo y conversión de escala son crecientes), así que basta una búsqueda
//! binaria sobre los valores que se pueden sacar (la escala con sus decimales). Esto da el
//! resultado exacto con o sin redondeo intermedio, con truncado o aproximado y con mezcla de
//! escalas, y la respuesta siempre es una nota que se puede sacar de verdad (4,1 y no 4,03).

use crate::calc::Calculator;
use crate::{GradeConfig, NodeId, Tree, TreeError};

/// Tolerancia al comparar con el objetivo (ruido de punto flotante).
const EPSILON: f64 = 1e-9;

/// Qué hace falta para llegar al objetivo de un nodo.
#[derive(Clone, Debug, PartialEq)]
pub enum Requirement {
    /// Necesitas sacar `grade` en lo que falta (la misma nota en todas si falta más de una).
    /// `grade` está en la escala de la primera evaluación que falta; `missing` trae la nota de
    /// cada una en su propia escala.
    Needed {
        grade: f64,
        missing: Vec<MissingGrade>,
    },
    /// El objetivo ya está asegurado: se cumple aunque lo que falta salga con la nota mínima.
    AlreadyMet,
    /// No se alcanza ni con la nota máxima en todo lo que falta (o ya no falta nada y no se
    /// cumplió).
    Unreachable,
}

/// Nota que hace falta en una evaluación sin nota.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MissingGrade {
    pub node: NodeId,
    /// En la escala de esa evaluación.
    pub grade: f64,
}

impl Tree {
    /// Nota mínima necesaria para llegar al objetivo de `id`. `Ok(None)` si el nodo no tiene
    /// objetivo.
    pub fn requirement(&self, id: NodeId) -> Result<Option<Requirement>, TreeError> {
        self.alive(id)?;
        let Some(target) = self.get(id).and_then(|n| n.target) else {
            return Ok(None);
        };

        let mut calc = Calculator::new(self);
        let missing: Vec<(NodeId, GradeConfig)> = self
            .missing_leaves(id)
            .into_iter()
            .map(|leaf| (leaf, calc.config(leaf)))
            .collect();

        let meets = |calc: &mut Calculator, step: Option<u64>| -> bool {
            calc.overrides.clear();
            if let Some(step) = step {
                for (leaf, grade) in assumed(&missing, step) {
                    calc.overrides.insert(leaf, grade);
                }
            }
            calc.probe(id).is_some_and(|g| g >= target - EPSILON)
        };

        let Some((_, first)) = missing.first() else {
            return Ok(Some(if meets(&mut calc, None) {
                Requirement::AlreadyMet
            } else {
                Requirement::Unreachable
            }));
        };

        let last = steps(first);
        if meets(&mut calc, Some(0)) {
            return Ok(Some(Requirement::AlreadyMet));
        }
        if !meets(&mut calc, Some(last)) {
            return Ok(Some(Requirement::Unreachable));
        }

        // Menor paso que cumple: `low` no cumple, `high` sí.
        let (mut low, mut high) = (0, last);
        while high - low > 1 {
            let mid = low + (high - low) / 2;
            if meets(&mut calc, Some(mid)) {
                high = mid;
            } else {
                low = mid;
            }
        }

        let missing: Vec<MissingGrade> = assumed(&missing, high)
            .map(|(node, grade)| MissingGrade { node, grade })
            .collect();
        Ok(Some(Requirement::Needed {
            grade: missing[0].grade,
            missing,
        }))
    }

    /// Hojas visibles sin nota dentro de `id` (incluido él mismo), en orden del árbol.
    fn missing_leaves(&self, id: NodeId) -> Vec<NodeId> {
        let mut found = Vec::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            if self.is_leaf(current) {
                if self.get(current).is_some_and(|n| n.grade.is_none()) {
                    found.push(current);
                }
            } else {
                let children: Vec<NodeId> = self.children(Some(current)).map(|c| c.id).collect();
                stack.extend(children.into_iter().rev());
            }
        }
        found
    }
}

/// Cantidad de pasos de la escala según sus decimales (de 1,0 a 7,0 con un decimal: 60).
fn steps(config: &GradeConfig) -> u64 {
    let factor = 10f64.powi(i32::from(config.decimals));
    (((config.max - config.min) * factor) - EPSILON)
        .ceil()
        .max(0.0) as u64
}

/// Notas supuestas en el paso `step` de la escala de la primera evaluación que falta. Las demás
/// van al mismo lugar relativo de su escala, subido al siguiente valor que se puede sacar.
fn assumed(
    missing: &[(NodeId, GradeConfig)],
    step: u64,
) -> impl Iterator<Item = (NodeId, f64)> + '_ {
    let fraction = match missing.first() {
        Some((_, first)) => {
            let total = steps(first);
            if total == 0 {
                1.0
            } else {
                step as f64 / total as f64
            }
        }
        None => 0.0,
    };
    missing.iter().map(move |(leaf, config)| {
        let raw = config.min + fraction * (config.max - config.min);
        (*leaf, ceil_to_scale(config, raw))
    })
}

/// El menor valor de la escala (según sus decimales, contando desde el mínimo) que es ≥ `grade`.
fn ceil_to_scale(config: &GradeConfig, grade: f64) -> f64 {
    let factor = 10f64.powi(i32::from(config.decimals));
    let k = ((grade - config.min) * factor - EPSILON).ceil().max(0.0);
    let value = config.min + k / factor;
    // Quitar el ruido binario (1,0 + 3,1 = 4,1000000000000005).
    let value = (value * 1e9).round() / 1e9;
    value.min(config.max)
}
