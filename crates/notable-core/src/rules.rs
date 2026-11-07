//! Reglas opcionales de cálculo. Todas vienen apagadas: un nodo con [`Rules::default`] se
//! calcula como en la Fase 2.
//!
//! Orden en que se aplican entre los hijos de un nodo:
//! 1. Reemplazos ([`Replacement`]): la nota de un hijo reemplaza a la de un hermano.
//! 2. Eliminar las peores ([`Rules::drop_lowest`], en el padre).
//! 3. Eximición ([`Exemption`]): con lo anterior se calcula la "nota de presentación"; si
//!    alcanza el umbral, la evaluación eximible no cuenta.
//! 4. Promedio.
//! 5. Requisitos mínimos ([`Minimum`]): si un hijo no llega a su mínimo, el padre queda
//!    reprobado y, si se configuró, su nota queda topada.

use serde::{Deserialize, Serialize};

use crate::NodeId;

/// Reglas de un nodo. Algunas se ponen en el padre (eliminar las peores) y otras en el hijo al
/// que afectan (reemplazo, eximición, mínimo).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    /// En el padre: cuántas de las peores notas de sus hijos se eliminan (0 = ninguna). Nunca se
    /// eliminan todas: siempre queda al menos una. No participan las evaluaciones eximibles ni
    /// las que reemplazan a otra.
    pub drop_lowest: u32,
    /// En el hijo: su nota reemplaza a la de un hermano.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaces: Option<Replacement>,
    /// En el hijo: no cuenta (no se rinde) si la nota de presentación alcanza un umbral.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exemption: Option<Exemption>,
    /// En el hijo: nota mínima que debe tener para aprobar el padre.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<Minimum>,
}

impl Rules {
    /// `true` si no hay ninguna regla activa.
    pub fn is_off(&self) -> bool {
        *self == Self::default()
    }
}

/// A quién reemplaza una nota (por ejemplo, un examen o una prueba recuperativa).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplaceTarget {
    /// La peor nota entre los hermanos que tienen nota.
    Lowest,
    /// Un hermano en particular. Si ese hermano no tiene nota, la toma (prueba recuperativa).
    Node(NodeId),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Replacement {
    pub target: ReplaceTarget,
    /// Reemplazar solo si mejora la nota (por defecto sí).
    pub only_if_better: bool,
    /// Si además cuenta como una nota más (por defecto no: solo sirve para reemplazar).
    pub counts_itself: bool,
}

impl Default for Replacement {
    fn default() -> Self {
        Self {
            target: ReplaceTarget::Lowest,
            only_if_better: true,
            counts_itself: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Exemption {
    /// Nota de presentación (promedio de los hermanos, en la escala del padre) desde la que se
    /// exime.
    pub threshold: f64,
    /// Si la evaluación igual se rindió (tiene nota) y subir el promedio, contarla.
    pub keep_if_better: bool,
}

impl Default for Exemption {
    fn default() -> Self {
        Self {
            threshold: 5.5,
            keep_if_better: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Minimum {
    /// Nota mínima del nodo, en su propia escala.
    pub grade: f64,
    /// Si no se cumple, la nota del padre queda en a lo más este valor (en la escala del padre).
    /// `None` = solo se marca como reprobado.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cap: Option<f64>,
}

impl Default for Minimum {
    fn default() -> Self {
        Self {
            grade: 4.0,
            cap: None,
        }
    }
}

/// Qué le hizo una regla a un nodo en el último cálculo (para mostrarlo en la interfaz).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Se eliminó por ser de las peores.
    Dropped,
    /// Su nota fue reemplazada por la de `by`.
    Replaced { by: NodeId },
    /// Su nota reemplazó a la de `node`.
    Replaces { node: NodeId },
    /// Es un reemplazo que no se usó (no mejoraba nada o no tenía a quién reemplazar).
    Unused,
    /// Eximido: no cuenta.
    Exempt,
}
