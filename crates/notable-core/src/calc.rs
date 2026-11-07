//! Motor de cálculo: promedio de cada nodo a partir de las notas ingresadas.
//!
//! - Promedio simple o ponderado según [`Aggregation`], ignorando los hijos sin nota
//!   (es la nota "parcial actual").
//! - Los pesos pueden expresarse respecto a un ancestro ("vale 20 % del ramo"); aquí se
//!   convierten a peso respecto al padre.
//! - Si un hijo usa otra escala, su nota se convierte a la del padre.
//! - Se redondea solo donde el usuario lo pide ([`crate::Node::round_result`] o
//!   [`GradeConfig::round_every_level`]); por defecto no se redondea ningún nivel.
//! - Las reglas opcionales ([`crate::Rules`]) se aplican al combinar los hijos.
//! - Los problemas de datos son avisos ([`Warning`]), nunca errores: siempre hay resultado.

use std::collections::HashMap;

use crate::{
    Aggregation, Effect, Exemption, GradeConfig, Minimum, NodeId, ReplaceTarget, Replacement, Tree,
};

/// Tolerancia para comparar sumas de pesos con 100.
const EPSILON: f64 = 1e-6;

/// Resultado del cálculo para un nodo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeResult {
    /// Nota del nodo en su propia escala: la ingresada (hoja) o el promedio de lo que hay, ya
    /// redondeado si corresponde. `None` si todavía no hay ninguna nota que la determine.
    pub grade: Option<f64>,
    /// La nota antes de redondear (igual a `grade` si no se redondea).
    pub unrounded: Option<f64>,
    /// `true` si todas las hojas que cuentan tienen nota (la nota ya es final). Las eximidas y
    /// los reemplazos que no cuentan como nota propia no hacen falta.
    pub complete: bool,
    /// `false` si algún hijo con nota no llega a su mínimo ([`crate::Minimum`]): el nodo queda
    /// reprobado aunque su promedio alcance.
    pub requirements_met: bool,
    /// Qué le hizo una regla opcional del padre a este nodo (eliminado, reemplazado, eximido…).
    pub effect: Option<Effect>,
    /// Peso del nodo dentro de su padre en %, ya convertido si se expresó respecto a otro
    /// ancestro. Es 0 si el padre pondera y el nodo no tiene peso. Para el promedio simple es
    /// 100 / cantidad de hermanos.
    pub weight: f64,
    /// Fracción (0–1) que vale el nodo dentro de su padre. Los nodos de nivel superior valen 1.
    pub share: f64,
    /// Configuración de nota que usa el nodo (propia o heredada).
    pub config: GradeConfig,
}

impl NodeResult {
    /// Aprobado: la nota alcanza la de aprobación y se cumplen los requisitos mínimos. `None` si
    /// todavía no hay nota.
    pub fn passed(&self) -> Option<bool> {
        self.grade
            .map(|g| g >= self.config.pass - GRADE_EPSILON && self.requirements_met)
    }
}

/// Un aviso del cálculo. No impide calcular; la interfaz lo muestra junto al nodo.
#[derive(Clone, Debug, PartialEq)]
pub enum Warning {
    /// Los pesos de los hijos de un nodo ponderado no suman 100 %.
    WeightsDontSum { node: NodeId, total: f64 },
    /// Igual que el anterior, pero hay pesos expresados respecto a un ancestro: los pesos de
    /// distintos niveles se contradicen (por ejemplo, las tareas valen 40 % del ramo, pero la
    /// suma de cada tarea "respecto al ramo" da 45 %).
    ContradictoryWeights { node: NodeId, total: f64 },
    /// Un hijo de un nodo ponderado no tiene peso; cuenta como 0.
    MissingWeight { node: NodeId },
    /// El nodo dice "respecto a" un nodo que ya no es su ancestro; se usó el padre.
    InvalidWeightBase { node: NodeId },
    /// El peso "respecto a" no se puede convertir porque algún nivel intermedio vale 0 %;
    /// se usó como peso respecto al padre.
    UnconvertibleWeight { node: NodeId },
    /// Nota ingresada fuera de la escala.
    GradeOutOfScale { node: NodeId, grade: f64 },
    /// Nota objetivo fuera de la escala.
    TargetOutOfScale { node: NodeId, target: f64 },
    /// Un hijo de un nodo que promedia por créditos no tiene créditos; cuenta como 0.
    MissingCredits { node: NodeId },
    /// El nodo reemplaza a otro que ya no es un hermano normal (se movió, se borró, o es
    /// eximible o reemplazo); no se reemplaza nada.
    InvalidReplaceTarget { node: NodeId },
    /// El nodo no llega a su nota mínima (`minimum`, en su escala): el padre queda reprobado.
    MinimumNotMet { node: NodeId, minimum: f64 },
}

impl Warning {
    /// Nodo al que se refiere el aviso.
    pub fn node(&self) -> NodeId {
        match self {
            Self::WeightsDontSum { node, .. }
            | Self::ContradictoryWeights { node, .. }
            | Self::MissingWeight { node }
            | Self::InvalidWeightBase { node }
            | Self::UnconvertibleWeight { node }
            | Self::GradeOutOfScale { node, .. }
            | Self::TargetOutOfScale { node, .. }
            | Self::MissingCredits { node }
            | Self::InvalidReplaceTarget { node }
            | Self::MinimumNotMet { node, .. } => *node,
        }
    }
}

/// Resultado de calcular todo el árbol.
#[derive(Clone, Debug, Default)]
pub struct Evaluation {
    results: HashMap<NodeId, NodeResult>,
    warnings: Vec<Warning>,
}

impl Evaluation {
    /// Resultado de un nodo visible. `None` si el nodo no existe o está borrado.
    pub fn get(&self, id: NodeId) -> Option<&NodeResult> {
        self.results.get(&id)
    }

    /// Atajo: la nota de un nodo.
    pub fn grade(&self, id: NodeId) -> Option<f64> {
        self.get(id).and_then(|r| r.grade)
    }

    /// Todos los avisos, en el orden en que se encontraron.
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// Avisos de un nodo.
    pub fn warnings_for(&self, id: NodeId) -> impl Iterator<Item = &Warning> {
        self.warnings.iter().filter(move |w| w.node() == id)
    }

    /// Fracción (0–1) que vale `node` dentro de `ancestor` ("esta evaluación vale 12 % del
    /// ramo"). `None` si `ancestor` no es ancestro de `node` (ni el mismo nodo).
    pub fn share_within(&self, tree: &Tree, node: NodeId, ancestor: NodeId) -> Option<f64> {
        let mut fraction = 1.0;
        let mut current = node;
        while current != ancestor {
            fraction *= self.get(current)?.share;
            current = tree.get(current)?.parent?;
        }
        Some(fraction)
    }
}

impl Tree {
    /// Calcula las notas de todos los nodos visibles.
    pub fn evaluate(&self) -> Evaluation {
        Calculator::new(self).run()
    }
}

/// Datos de un nodo que se resuelven de arriba hacia abajo (antes de promediar).
#[derive(Clone, Copy)]
struct Layout {
    config: GradeConfig,
    weight: f64,
    share: f64,
}

/// Calcula notas. La disposición (configuración, pesos, fracciones) no depende de las notas, así
/// que se resuelve una vez en [`Calculator::new`]; luego [`Calculator::compute`] puede llamarse
/// varias veces con distintas notas supuestas (`overrides`), como hace la nota mínima.
pub(crate) struct Calculator<'a> {
    tree: &'a Tree,
    layout: HashMap<NodeId, Layout>,
    /// Notas supuestas para hojas sin nota (no se guardan en el árbol).
    pub(crate) overrides: HashMap<NodeId, f64>,
    results: HashMap<NodeId, NodeResult>,
    warnings: Vec<Warning>,
}

impl<'a> Calculator<'a> {
    pub(crate) fn new(tree: &'a Tree) -> Self {
        let mut calc = Self {
            tree,
            layout: HashMap::new(),
            overrides: HashMap::new(),
            results: HashMap::new(),
            warnings: Vec::new(),
        };

        // De arriba hacia abajo: configuración, pesos y fracciones. Cada nivel necesita las
        // fracciones de sus ancestros para convertir pesos "respecto a".
        let mut queue = Vec::new();
        for root in tree.roots() {
            let config = root.config.unwrap_or_default();
            calc.layout.insert(
                root.id,
                Layout {
                    config,
                    weight: 100.0,
                    share: 1.0,
                },
            );
            queue.push(root.id);
        }
        let mut i = 0;
        while i < queue.len() {
            let parent = queue[i];
            i += 1;
            calc.layout_children(parent);
            queue.extend(tree.children(Some(parent)).map(|c| c.id));
        }
        calc
    }

    /// Configuración que usa un nodo visible.
    pub(crate) fn config(&self, id: NodeId) -> GradeConfig {
        self.layout[&id].config
    }

    /// Calcula `id` con las notas supuestas actuales. Los resultados del subárbol quedan
    /// disponibles (ver [`Calculator::effect`]) hasta el siguiente cálculo; los avisos se
    /// descartan.
    pub(crate) fn probe(&mut self, id: NodeId) -> NodeResult {
        self.results.clear();
        let result = self.compute(id);
        self.warnings.clear();
        result
    }

    fn run(mut self) -> Evaluation {
        // De abajo hacia arriba: notas.
        let roots: Vec<NodeId> = self.tree.roots().map(|n| n.id).collect();
        for root in roots {
            self.compute(root);
        }

        Evaluation {
            results: self.results,
            warnings: self.warnings,
        }
    }

    /// Resuelve configuración, peso y fracción de los hijos de `parent`.
    fn layout_children(&mut self, parent: NodeId) {
        let parent_node = self.tree.get(parent).expect("visible node");
        let parent_config = self.layout[&parent].config;
        let children: Vec<_> = self.tree.children(Some(parent)).collect();
        if children.is_empty() {
            return;
        }

        let weighted = parent_node.aggregation == Aggregation::Weighted;
        let mut weights = Vec::with_capacity(children.len());
        let mut uses_base = false;

        for child in &children {
            let weight = match parent_node.aggregation {
                Aggregation::Weighted => match child.weight {
                    Some(w) => self.convert_weight(child.id, parent, w, &mut uses_base),
                    None => {
                        self.warnings
                            .push(Warning::MissingWeight { node: child.id });
                        0.0
                    }
                },
                Aggregation::Credits => match self.tree.effective_credits(child.id) {
                    Some(c) => c,
                    None => {
                        self.warnings
                            .push(Warning::MissingCredits { node: child.id });
                        0.0
                    }
                },
                Aggregation::Simple => 100.0 / children.len() as f64,
            };
            weights.push(weight);
        }

        let mut total: f64 = weights.iter().sum();
        if parent_node.aggregation == Aggregation::Credits && total > 0.0 {
            // Los créditos se expresan como % para que `weight` signifique lo mismo siempre.
            for w in &mut weights {
                *w = *w * 100.0 / total;
            }
            total = 100.0;
        }
        if weighted && (total - 100.0).abs() > EPSILON {
            self.warnings.push(if uses_base {
                Warning::ContradictoryWeights {
                    node: parent,
                    total,
                }
            } else {
                Warning::WeightsDontSum {
                    node: parent,
                    total,
                }
            });
        }

        for (child, weight) in children.iter().zip(weights) {
            let share = if total > 0.0 { weight / total } else { 0.0 };
            self.layout.insert(
                child.id,
                Layout {
                    config: child.config.unwrap_or(parent_config),
                    weight,
                    share,
                },
            );
        }
    }

    /// Pasa un peso "respecto a" un ancestro a peso respecto al padre.
    fn convert_weight(
        &mut self,
        id: NodeId,
        parent: NodeId,
        weight: f64,
        uses_base: &mut bool,
    ) -> f64 {
        let Some(base) = self.tree.get(id).and_then(|n| n.weight_base) else {
            return weight;
        };
        if base == parent {
            return weight;
        }
        if !self.tree.ancestors(parent).any(|a| a == base) {
            self.warnings.push(Warning::InvalidWeightBase { node: id });
            return weight;
        }

        // Fracción que vale el padre dentro de `base`.
        let mut fraction = 1.0;
        let mut current = parent;
        while current != base {
            fraction *= self.layout[&current].share;
            current = self
                .tree
                .get(current)
                .and_then(|n| n.parent)
                .expect("below base");
        }
        if fraction <= 0.0 {
            self.warnings
                .push(Warning::UnconvertibleWeight { node: id });
            return weight;
        }
        *uses_base = true;
        weight / fraction
    }

    /// Calcula la nota de `id` (y de todo lo que cuelga de él).
    fn compute(&mut self, id: NodeId) -> NodeResult {
        let node = self.tree.get(id).expect("visible node");
        let layout = self.layout[&id];
        let config = layout.config;
        let children: Vec<NodeId> = self.tree.children(Some(id)).map(|c| c.id).collect();

        if let Some(t) = node.target
            && !config.contains(t)
        {
            self.warnings.push(Warning::TargetOutOfScale {
                node: id,
                target: t,
            });
        }

        let combined = if children.is_empty() {
            if let Some(g) = node.grade
                && !config.contains(g)
            {
                self.warnings
                    .push(Warning::GradeOutOfScale { node: id, grade: g });
            }
            let grade = node.grade.or_else(|| self.overrides.get(&id).copied());
            Combined {
                average: grade,
                complete: grade.is_some(),
                requirements_met: true,
                cap: None,
            }
        } else {
            self.combine(id, &children, &config)
        };
        let unrounded = combined.average;

        let is_computed = !self.tree.is_leaf(id);
        // Un nodo con objetivo redondea su nota final (salvo que diga lo contrario), porque es la
        // que se compara con el objetivo.
        let round = is_computed
            && node
                .round_result
                .unwrap_or(config.round_every_level || node.target.is_some());
        let mut grade = if round {
            unrounded.map(|g| config.apply_rounding(g))
        } else {
            unrounded
        };
        let mut unrounded = unrounded;
        if let Some(cap) = combined.cap {
            grade = grade.map(|g| g.min(cap));
            unrounded = unrounded.map(|g| g.min(cap));
        }

        let result = NodeResult {
            grade,
            unrounded,
            complete: combined.complete,
            requirements_met: combined.requirements_met,
            effect: None,
            weight: layout.weight,
            share: layout.share,
            config,
        };
        self.results.insert(id, result);
        result
    }

    /// Combina las notas de los hijos de `parent` aplicando las reglas opcionales (ver
    /// [`crate::rules`] para el orden).
    fn combine(&mut self, parent: NodeId, children: &[NodeId], config: &GradeConfig) -> Combined {
        let tree = self.tree;
        let mut regular: Vec<Entry> = Vec::new();
        let mut replacers: Vec<(Entry, Replacement)> = Vec::new();
        let mut exemptable: Vec<(Entry, Exemption)> = Vec::new();
        // Hijos con requisito mínimo: (id, mínimo en la escala del padre, regla, requisitos
        // del hijo cumplidos).
        let mut minimums: Vec<(NodeId, f64, Minimum, bool)> = Vec::new();

        for &child in children {
            let r = self.compute(child);
            let rules = tree.get(child).expect("visible node").rules;
            if let Some(min) = rules.minimum {
                let in_parent = r.config.convert_to(min.grade, config);
                minimums.push((child, in_parent, min, r.requirements_met));
            }
            let entry = Entry {
                id: child,
                grade: r.grade.map(|g| r.config.convert_to(g, config)),
                weight: r.weight,
                complete: r.complete,
            };
            match (rules.replaces, rules.exemption) {
                (Some(rep), _) => replacers.push((entry, rep)),
                (None, Some(ex)) => exemptable.push((entry, ex)),
                (None, None) => regular.push(entry),
            }
        }

        // 1. Reemplazos.
        let mut replaced = vec![false; regular.len()];
        let mut counted: Vec<Entry> = Vec::new();
        let mut complete = true;
        for (entry, rep) in &replacers {
            if rep.counts_itself {
                complete &= entry.complete;
                counted.push(*entry);
            }
            let Some(g) = entry.grade else { continue };
            let index = match rep.target {
                ReplaceTarget::Lowest => regular
                    .iter()
                    .enumerate()
                    .filter(|(i, e)| !replaced[*i] && e.grade.is_some())
                    .min_by(|(_, a), (_, b)| a.grade.partial_cmp(&b.grade).expect("finite"))
                    .map(|(i, _)| i),
                ReplaceTarget::Node(target) => match regular.iter().position(|e| e.id == target) {
                    Some(i) if !replaced[i] => Some(i),
                    Some(_) => None,
                    None => {
                        self.warnings
                            .push(Warning::InvalidReplaceTarget { node: entry.id });
                        None
                    }
                },
            };
            let applied = index.filter(|&i| match regular[i].grade {
                None => true,
                Some(old) => !rep.only_if_better || g > old + GRADE_EPSILON,
            });
            match applied {
                Some(i) => {
                    let target = &mut regular[i];
                    target.grade = Some(g);
                    target.complete = entry.complete;
                    replaced[i] = true;
                    self.set_effect(target.id, Effect::Replaced { by: entry.id });
                    let target = target.id;
                    self.set_effect(entry.id, Effect::Replaces { node: target });
                }
                None => self.set_effect(entry.id, Effect::Unused),
            }
        }

        // 2. Eliminar las peores (al menos una nota queda).
        let drop = tree.get(parent).expect("visible node").rules.drop_lowest as usize;
        let mut graded: Vec<usize> = (0..regular.len())
            .filter(|&i| regular[i].grade.is_some())
            .collect();
        let drop = drop.min(graded.len().saturating_sub(1));
        graded.sort_by(|&a, &b| {
            regular[a]
                .grade
                .partial_cmp(&regular[b].grade)
                .expect("finite")
                .then(a.cmp(&b))
        });
        let dropped: Vec<usize> = graded.into_iter().take(drop).collect();
        for (i, entry) in regular.iter().enumerate() {
            complete &= entry.complete;
            if dropped.contains(&i) {
                self.set_effect(entry.id, Effect::Dropped);
            } else {
                counted.push(*entry);
            }
        }

        // 3. Eximición, según la nota de presentación (lo anterior).
        let presentation = average(&counted);
        for (entry, ex) in &exemptable {
            let exempt = presentation.is_some_and(|p| p >= ex.threshold - GRADE_EPSILON);
            let keep = !exempt
                || (ex.keep_if_better
                    && entry.grade.is_some()
                    && average(&[counted.as_slice(), &[*entry]].concat()) > average(&counted));
            if keep {
                complete &= entry.complete;
                counted.push(*entry);
            } else {
                self.set_effect(entry.id, Effect::Exempt);
            }
        }

        // 4. Promedio.
        let avg = average(&counted);

        // 5. Requisitos mínimos (con la nota ya reemplazada, si se reemplazó).
        let effective: HashMap<NodeId, Option<f64>> = regular
            .iter()
            .chain(replacers.iter().map(|(e, _)| e))
            .chain(exemptable.iter().map(|(e, _)| e))
            .map(|e| (e.id, e.grade))
            .collect();
        let mut requirements_met = true;
        let mut cap: Option<f64> = None;
        for (child, minimum, rule, child_met) in minimums {
            if self.results.get(&child).and_then(|r| r.effect) == Some(Effect::Exempt) {
                continue;
            }
            let Some(g) = effective[&child] else { continue };
            if g < minimum - GRADE_EPSILON || !child_met {
                requirements_met = false;
                self.warnings.push(Warning::MinimumNotMet {
                    node: child,
                    minimum: rule.grade,
                });
                if let Some(c) = rule.cap {
                    cap = Some(cap.map_or(c, |old: f64| old.min(c)));
                }
            }
        }

        Combined {
            average: avg,
            complete,
            requirements_met,
            cap,
        }
    }

    fn set_effect(&mut self, id: NodeId, effect: Effect) {
        if let Some(r) = self.results.get_mut(&id) {
            r.effect = Some(effect);
        }
    }

    /// Efecto de una regla sobre un nodo en el último cálculo.
    pub(crate) fn effect(&self, id: NodeId) -> Option<Effect> {
        self.results.get(&id).and_then(|r| r.effect)
    }
}

/// Tolerancia para comparar notas (ruido de punto flotante).
const GRADE_EPSILON: f64 = 1e-9;

/// Un hijo, con su nota ya en la escala del padre.
#[derive(Clone, Copy)]
struct Entry {
    id: NodeId,
    grade: Option<f64>,
    weight: f64,
    complete: bool,
}

/// Resultado de combinar los hijos de un nodo (antes de redondear).
struct Combined {
    average: Option<f64>,
    complete: bool,
    requirements_met: bool,
    /// Tope por requisitos mínimos no cumplidos.
    cap: Option<f64>,
}

/// Promedio ponderado de las entradas con nota.
fn average(entries: &[Entry]) -> Option<f64> {
    let (sum, weight) = entries
        .iter()
        .filter_map(|e| e.grade.map(|g| (e.weight * g, e.weight)))
        .fold((0.0, 0.0), |(s, w), (eg, ew)| (s + eg, w + ew));
    (weight > 0.0).then(|| sum / weight)
}
