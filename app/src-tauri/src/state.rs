//! Estado de la app: el árbol en memoria y la base local. Cada cambio se guarda al instante.
//!
//! No depende de Tauri, así se prueba directo.

use std::path::Path;

use notable_api::{
    Change, ConfigView, Crumb, Destination, Details, ExemptionSettings, FactorSettings, Goal,
    GoalKind, Item, MinimumSettings, Mode, ReplaceSettings, RulesSettings, Scale, ScaleSettings,
    Screen, format_grade, format_number,
};
use notable_core::{
    Aggregation, Conversion, Effect, Evaluation, Exemption, Factor, GradeConfig, Minimum, NodeId,
    ReplaceTarget, Replacement, Requirement, Rounding, Rules, Tree, Warning,
};
use notable_store::{Store, SyncRow};
use notable_sync::{RemoteRow, Session};

pub type Result<T> = std::result::Result<T, String>;

pub struct Data {
    tree: Tree,
    store: Store,
}

impl Data {
    /// Abre la base en `path` y carga el árbol.
    pub fn open(path: &Path) -> Result<Self> {
        Self::from_store(Store::open(path).map_err(|e| e.to_string())?)
    }

    /// Base en memoria (para pruebas).
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        Self::from_store(Store::open_in_memory().map_err(|e| e.to_string())?)
    }

    fn from_store(store: Store) -> Result<Self> {
        let tree = store.load_tree().map_err(|e| e.to_string())?;
        Ok(Self { tree, store })
    }

    /// Pantalla de `view` (`None` = inicio). Si el nodo ya no está visible (por ejemplo, se
    /// borró), muestra el ancestro visible más cercano.
    pub fn screen(&self, view: Option<&str>) -> Result<Screen> {
        let mut current = view.map(parse_id).transpose()?;
        while let Some(id) = current {
            if self.tree.is_alive(id) {
                break;
            }
            current = self.tree.get(id).and_then(|n| n.parent);
        }

        let eval = self.tree.evaluate();
        let path = match current {
            Some(id) => {
                let mut ids: Vec<NodeId> = self.tree.ancestors(id).collect();
                ids.reverse();
                ids.push(id);
                ids.into_iter().map(|id| self.crumb(id)).collect()
            }
            None => Vec::new(),
        };
        Ok(Screen {
            path,
            current: current.map(|id| self.item(&eval, id)),
            details: current.map(|id| self.details(&eval, id)).transpose()?,
            children: self
                .tree
                .children(current)
                .map(|c| self.item(&eval, c.id))
                .collect(),
        })
    }

    /// Crea un nodo al final de `parent` y devuelve su id.
    pub fn create(&mut self, parent: Option<&str>, name: &str) -> Result<String> {
        let parent = parent.map(parse_id).transpose()?;
        let id = self.tree.create(parent, name).map_err(|e| e.to_string())?;
        self.save()?;
        Ok(id.to_string())
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<()> {
        let id = parse_id(id)?;
        self.tree.rename(id, name).map_err(|e| e.to_string())?;
        self.save()
    }

    /// Borrado lógico; se puede deshacer con [`Data::restore`].
    pub fn delete(&mut self, id: &str) -> Result<()> {
        let id = parse_id(id)?;
        self.tree.delete(id).map_err(|e| e.to_string())?;
        self.save()
    }

    pub fn restore(&mut self, id: &str) -> Result<()> {
        let id = parse_id(id)?;
        self.tree.restore(id).map_err(|e| e.to_string())?;
        self.save()
    }

    /// Ingresa o borra (`None`) la nota de una hoja.
    pub fn set_grade(&mut self, id: &str, grade: Option<f64>) -> Result<()> {
        let id = parse_id(id)?;
        self.tree.set_grade(id, grade).map_err(|e| e.to_string())?;
        self.save()
    }

    /// Mueve un nodo una posición arriba (`-1`) o abajo (`+1`) entre sus hermanos.
    pub fn shift(&mut self, id: &str, delta: i32) -> Result<()> {
        let id = parse_id(id)?;
        let parent = self.tree.get(id).ok_or("no existe el nodo")?.parent;
        let siblings: Vec<NodeId> = self.tree.children(parent).map(|c| c.id).collect();
        let index = siblings
            .iter()
            .position(|s| *s == id)
            .ok_or("nodo oculto")?;
        let target = index as i64 + i64::from(delta);
        if target < 0 || target >= siblings.len() as i64 {
            return Ok(());
        }
        self.tree
            .move_node(id, parent, target as usize)
            .map_err(|e| e.to_string())?;
        self.save()
    }

    /// Lugares a los que se puede mover `id`: el inicio y todos los nodos visibles, salvo él
    /// mismo y lo que tiene adentro. En orden del árbol, con su profundidad.
    pub fn destinations(&self, id: &str) -> Result<Vec<Destination>> {
        let id = parse_id(id)?;
        let parent = self.tree.get(id).ok_or("no existe el nodo")?.parent;
        let mut list = vec![Destination {
            id: None,
            name: "Inicio".to_owned(),
            depth: 0,
            current: parent.is_none(),
            has_grade: false,
        }];
        // Recorrido en profundidad, sin entrar al propio nodo.
        let mut stack: Vec<(NodeId, usize)> = self.tree.roots().map(|n| (n.id, 1)).collect();
        stack.reverse();
        while let Some((node, depth)) = stack.pop() {
            if node == id {
                continue;
            }
            let leaf = self.tree.is_leaf(node);
            list.push(Destination {
                id: Some(node.to_string()),
                name: self.name(node),
                depth,
                current: parent == Some(node),
                has_grade: leaf && self.tree.get(node).is_some_and(|n| n.grade.is_some()),
            });
            let children: Vec<NodeId> = self.tree.children(Some(node)).map(|c| c.id).collect();
            stack.extend(children.into_iter().rev().map(|c| (c, depth + 1)));
        }
        Ok(list)
    }

    /// Mueve `id` al final de `parent` (`None` = inicio), con todo lo que tiene adentro.
    pub fn move_to(&mut self, id: &str, parent: Option<&str>) -> Result<()> {
        let id = parse_id(id)?;
        let parent = parent.map(parse_id).transpose()?;
        if self.tree.get(id).ok_or("no existe el nodo")?.parent == parent {
            return Ok(());
        }
        self.tree
            .move_node(id, parent, usize::MAX)
            .map_err(|e| match e {
                notable_core::TreeError::Cycle(_) => {
                    "no se puede mover algo dentro de sí mismo".to_owned()
                }
                e => e.to_string(),
            })?;
        self.save()
    }

    /// Aplica un cambio de ajustes a un nodo.
    pub fn update(&mut self, id: &str, change: Change) -> Result<()> {
        let id = parse_id(id)?;
        let tree = &mut self.tree;
        let result = match change {
            Change::Mode(mode) => {
                let aggregation = match mode {
                    Mode::Simple => Aggregation::Simple,
                    Mode::Weighted => Aggregation::Weighted,
                    Mode::Credits => Aggregation::Credits,
                };
                // Al ponderar por primera vez, repartir 100 % en partes iguales para partir.
                if aggregation == Aggregation::Weighted {
                    // Los factores no llevan peso: no entran al reparto.
                    let children: Vec<NodeId> = tree
                        .children(Some(id))
                        .filter(|c| c.rules.factor.is_none())
                        .map(|c| c.id)
                        .collect();
                    let unweighted = children
                        .iter()
                        .all(|c| tree.get(*c).is_some_and(|n| n.weight.is_none()));
                    if unweighted && !children.is_empty() {
                        let each = (10_000.0 / children.len() as f64).floor() / 100.0;
                        let last = 100.0 - each * (children.len() - 1) as f64;
                        for (i, child) in children.iter().enumerate() {
                            let w = if i + 1 == children.len() { last } else { each };
                            let w = (w * 100.0).round() / 100.0;
                            tree.set_weight(*child, Some(w))
                                .map_err(|e| e.to_string())?;
                        }
                    }
                }
                tree.set_aggregation(id, aggregation)
            }
            Change::Weight(weight) => tree.set_weight(id, weight),
            Change::WeightBase(base) => {
                let base = base.as_deref().map(parse_id).transpose()?;
                tree.set_weight_base(id, base)
            }
            Change::Config(settings) => tree.set_config(id, settings.map(to_config)),
            Change::Target(target) => tree.set_target(id, target),
            Change::RoundResult(round) => tree.set_round_result(id, round),
            Change::Credits(credits) => tree.set_credits(id, credits),
            Change::Rules(rules) => {
                let rules = to_rules(&rules)?;
                tree.set_rules(id, rules)
            }
        };
        if let Err(e) = result {
            // Lo que alcanzó a cambiar (pesos repartidos) se guarda igual.
            self.save()?;
            return Err(e.to_string());
        }
        self.save()
    }

    // ---------------------------------------------------------------- cuenta y sincronización
    //
    // La red se usa fuera del candado del estado: la app llama a `sync_begin`, baja los cambios
    // sin bloquear nada, llama a `sync_apply` (que devuelve lo pendiente), sube y termina con
    // `sync_end`.

    /// La sesión guardada (si se inició sesión en este dispositivo).
    pub fn session(&self) -> Result<Option<Session>> {
        notable_sync::load_session(&self.store).map_err(|e| e.to_string())
    }

    /// Guarda la sesión recién iniciada y prepara la fusión con la cuenta.
    pub fn signed_in(&mut self, session: &Session) -> Result<()> {
        notable_sync::bind_account(&mut self.store, &session.user_id).map_err(|e| e.to_string())?;
        self.store_session(Some(session))
    }

    /// Guarda (o borra) la sesión. Cerrar sesión no borra los datos locales.
    pub fn store_session(&mut self, session: Option<&Session>) -> Result<()> {
        notable_sync::save_session(&mut self.store, session).map_err(|e| e.to_string())
    }

    /// Hora de la última sincronización completa.
    pub fn last_sync(&self) -> Option<i64> {
        notable_sync::last_sync(&self.store).ok().flatten()
    }

    /// Paso 1: sesión y desde cuándo bajar. `None` si no hay sesión.
    pub fn sync_begin(&self) -> Result<Option<(Session, Option<i64>)>> {
        let Some(session) = self.session()? else {
            return Ok(None);
        };
        let since = notable_sync::pull_cursor(&self.store).map_err(|e| e.to_string())?;
        Ok(Some((session, since)))
    }

    /// Paso 2: aplica lo que bajó (recargando el árbol si algo cambió) y devuelve lo pendiente
    /// de subir y cuántas filas cambiaron aquí.
    pub fn sync_apply(&mut self, rows: &[RemoteRow]) -> Result<(Vec<SyncRow>, usize)> {
        let applied =
            notable_sync::finish_pull(&mut self.store, rows).map_err(|e| e.to_string())?;
        if applied > 0 {
            self.tree = self.store.load_tree().map_err(|e| e.to_string())?;
        }
        let pending = self.store.pending().map_err(|e| e.to_string())?;
        Ok((pending, applied))
    }

    /// Paso 3: marca como subido lo que se subió.
    pub fn sync_end(&mut self, pushed: &[SyncRow], now: i64) -> Result<()> {
        notable_sync::finish_push(&mut self.store, pushed, now).map_err(|e| e.to_string())
    }

    /// Cantidad de cambios sin subir.
    pub fn pending_count(&self) -> usize {
        self.store.pending().map_or(0, |p| p.len())
    }

    // ---------------------------------------------------------------- respaldo

    /// Respaldo en JSON de todo (incluidos los borrados, para poder restaurarlos).
    pub fn export_backup(&self) -> Result<String> {
        self.store
            .export_json()
            .map_err(|e| format!("no se pudo crear el respaldo: {e}"))
    }

    /// Importa un respaldo **sin borrar nada**: lo del respaldo reemplaza a lo que tenga el mismo
    /// id y lo demás se conserva. Devuelve cuántas cosas trae el respaldo.
    pub fn import_backup(&mut self, json: &str) -> Result<usize> {
        let count = notable_store::Backup::from_json(json)
            .map_err(|e| format!("no se pudo importar: {e}"))?
            .nodes
            .len();
        // Si el resultado no es válido, la base no cambia. Lo importado queda pendiente de
        // sincronizar (si hay cuenta).
        self.tree = self
            .store
            .import_json(json)
            .map_err(|e| format!("no se pudo importar: {e}"))?;
        Ok(count)
    }

    fn save(&mut self) -> Result<()> {
        self.store
            .save(&mut self.tree)
            .map(|_| ())
            .map_err(|e| format!("no se pudo guardar: {e}"))
    }

    fn name(&self, id: NodeId) -> String {
        self.tree
            .get(id)
            .map_or_else(|| "?".to_owned(), |n| n.name.clone())
    }

    fn crumb(&self, id: NodeId) -> Crumb {
        Crumb {
            id: id.to_string(),
            name: self.name(id),
        }
    }

    fn item(&self, eval: &Evaluation, id: NodeId) -> Item {
        let node = self.tree.get(id).expect("nodo visible");
        let result = eval.get(id).expect("nodo visible");
        let config = result.config;

        // "= 25 % de Tareas" cuando el peso se expresó respecto a otro nivel.
        let weight_note = match (node.weight_base, node.parent) {
            (Some(base), Some(parent)) if base != parent && node.weight.is_some() => Some(format!(
                "= {} % de {}",
                format_number(result.weight),
                self.name(parent)
            )),
            _ => None,
        };

        let mut badges = Vec::new();
        let rules = &node.rules;
        if rules.drop_lowest > 0 {
            badges.push(if rules.drop_lowest == 1 {
                "Elimina la peor".to_owned()
            } else {
                format!("Elimina las {} peores", rules.drop_lowest)
            });
        }
        if let Some(rep) = rules.replaces {
            badges.push(match rep.target {
                ReplaceTarget::Lowest => "Reemplaza a la peor".to_owned(),
                ReplaceTarget::Node(t) => format!("Reemplaza a {}", self.name(t)),
            });
        }
        if let Some(ex) = rules.exemption {
            let parent_config = node
                .parent
                .and_then(|p| eval.get(p))
                .map_or(config, |r| r.config);
            badges.push(format!(
                "Eximible desde {}",
                format_grade(ex.threshold, parent_config.decimals)
            ));
        }
        if let Some(min) = rules.minimum {
            badges.push(format!(
                "Mínimo {}",
                format_grade(min.grade, config.decimals)
            ));
        }
        if let Some(f) = rules.factor {
            badges.push(format!(
                "Factor {} a {}",
                format_number(f.min),
                format_number(f.max)
            ));
        }

        let effect = result.effect.map(|e| match e {
            Effect::Dropped => "Eliminada (de las peores)".to_owned(),
            Effect::Replaced { by } => format!("Reemplazada por {}", self.name(by)),
            Effect::Replaces { node } => format!("Reemplaza a {}", self.name(node)),
            Effect::Unused => "No se usó".to_owned(),
            Effect::Exempt => "Eximida".to_owned(),
        });
        // El factor no es un efecto de una regla del padre, pero se muestra igual.
        let effect = effect.or_else(|| {
            result
                .factor
                .map(|f| format!("Multiplica la nota por {}", format_number(f)))
        });

        Item {
            id: id.to_string(),
            name: node.name.clone(),
            is_leaf: self.tree.is_leaf(id),
            grade: node.grade,
            value: result.grade,
            display: result.grade.map_or_else(
                || "—".to_owned(),
                |g| format_grade(config.apply_rounding(g), config.decimals),
            ),
            complete: result.complete,
            passed: result.passed(),
            children: self.tree.children(Some(id)).count(),
            scale: Scale {
                min: config.min,
                max: config.max,
                decimals: config.decimals,
            },
            weight: node.weight,
            weight_base: node.weight_base.map(|b| b.to_string()),
            weight_note,
            credits: node.credits,
            is_factor: node.rules.factor.is_some(),
            effect,
            badges,
            warnings: eval
                .warnings_for(id)
                .map(|w| self.warning_text(w, &config))
                .collect(),
        }
    }

    fn details(&self, eval: &Evaluation, id: NodeId) -> Result<Details> {
        let node = self.tree.get(id).expect("nodo visible");
        let result = eval.get(id).expect("nodo visible");
        let source = self.tree.config_source(id).map_err(|e| e.to_string())?;
        let config = result.config;

        let goal = self
            .tree
            .requirement(id)
            .map_err(|e| e.to_string())?
            .map(|r| self.goal(r, result.complete));

        let rules = &node.rules;
        let rules = RulesSettings {
            drop_lowest: rules.drop_lowest,
            replaces: rules.replaces.map(|r| ReplaceSettings {
                target: match r.target {
                    ReplaceTarget::Lowest => None,
                    ReplaceTarget::Node(t) => Some(t.to_string()),
                },
                only_if_better: r.only_if_better,
                counts_itself: r.counts_itself,
            }),
            exemption: rules.exemption.map(|e| ExemptionSettings {
                threshold: e.threshold,
                keep_if_better: e.keep_if_better,
            }),
            minimum: rules.minimum.map(|m| MinimumSettings {
                grade: m.grade,
                cap: m.cap,
            }),
            factor: rules.factor.map(|f| FactorSettings {
                min: f.min,
                max: f.max,
                neutral: f.neutral,
            }),
        };

        Ok(Details {
            mode: match node.aggregation {
                Aggregation::Simple => Mode::Simple,
                Aggregation::Weighted => Mode::Weighted,
                Aggregation::Credits => Mode::Credits,
            },
            config: ConfigView {
                own: source == Some(id),
                inherited_from: source.filter(|s| *s != id).map(|s| self.name(s)),
                settings: to_settings(&config),
            },
            round_result: node.round_result,
            target: node.target,
            goal,
            credits: node.credits,
            child_credits: if node.credits.is_none() {
                self.tree.effective_credits(id)
            } else {
                None
            },
            rules,
            siblings: self
                .tree
                .children(node.parent)
                .filter(|s| s.id != id)
                .map(|s| self.crumb(s.id))
                .collect(),
            has_parent: node.parent.is_some(),
            warnings: eval
                .warnings_for(id)
                .map(|w| self.warning_text(w, &config))
                .collect(),
        })
    }

    fn goal(&self, requirement: Requirement, complete: bool) -> Goal {
        match requirement {
            Requirement::Needed { grade, missing } => {
                let decimals = |id: NodeId| {
                    self.tree
                        .effective_config(id)
                        .map_or(1, |c: GradeConfig| c.decimals)
                };
                let first = missing.first().map(|m| m.node);
                let title = format!(
                    "Necesitas {}",
                    format_grade(grade, first.map_or(1, decimals))
                );
                let same_value = missing.iter().all(|m| (m.grade - grade).abs() < 1e-9);
                let detail = if missing.len() == 1 {
                    format!("en {}", self.name(missing[0].node))
                } else if same_value {
                    let names: Vec<String> = missing.iter().map(|m| self.name(m.node)).collect();
                    format!(
                        "en cada una de las {} que faltan: {}",
                        missing.len(),
                        names.join(", ")
                    )
                } else {
                    let parts: Vec<String> = missing
                        .iter()
                        .map(|m| {
                            format!(
                                "{} ({})",
                                self.name(m.node),
                                format_grade(m.grade, decimals(m.node))
                            )
                        })
                        .collect();
                    format!("en lo que falta: {}", parts.join(", "))
                };
                Goal {
                    kind: GoalKind::Needed,
                    title,
                    detail,
                }
            }
            Requirement::AlreadyMet => Goal {
                kind: GoalKind::Met,
                title: if complete {
                    "Objetivo cumplido".to_owned()
                } else {
                    "Objetivo asegurado".to_owned()
                },
                detail: if complete {
                    "Llegaste al objetivo.".to_owned()
                } else {
                    "Lo logras aunque saques la nota mínima en lo que falta.".to_owned()
                },
            },
            Requirement::Unreachable => Goal {
                kind: GoalKind::Unreachable,
                title: "No alcanzable".to_owned(),
                detail: if complete {
                    "Ya no quedan notas por ingresar.".to_owned()
                } else {
                    "Ni con la nota máxima en todo lo que falta.".to_owned()
                },
            },
        }
    }

    fn warning_text(&self, warning: &Warning, config: &GradeConfig) -> String {
        let grade = |g: f64| format_grade(g, config.decimals);
        match warning {
            Warning::WeightsDontSum { total, .. } => {
                format!("Los pesos suman {} %, no 100 %.", format_number(*total))
            }
            Warning::ContradictoryWeights { total, .. } => format!(
                "Los pesos de distintos niveles no cuadran: suman {} % aquí.",
                format_number(*total)
            ),
            Warning::MissingWeight { .. } => "No tiene peso: cuenta 0 %.".to_owned(),
            Warning::InvalidWeightBase { .. } => {
                "Su peso era respecto a un nivel que ya no lo contiene; se usa respecto al padre."
                    .to_owned()
            }
            Warning::UnconvertibleWeight { .. } => {
                "Su peso no se puede convertir: un nivel intermedio vale 0 %.".to_owned()
            }
            Warning::GradeOutOfScale { grade: g, .. } => {
                format!("La nota {} está fuera de la escala.", grade(*g))
            }
            Warning::TargetOutOfScale { target, .. } => {
                format!("El objetivo {} está fuera de la escala.", grade(*target))
            }
            Warning::MissingCredits { .. } => "No tiene créditos: cuenta 0.".to_owned(),
            Warning::InvalidReplaceTarget { .. } => {
                "Reemplaza a una evaluación que ya no está; no reemplaza nada.".to_owned()
            }
            Warning::MinimumNotMet { minimum, .. } => {
                format!(
                    "No llega al mínimo de {}: reprueba el nivel de arriba.",
                    grade(*minimum)
                )
            }
        }
    }
}

fn to_settings(config: &GradeConfig) -> ScaleSettings {
    ScaleSettings {
        min: config.min,
        max: config.max,
        pass: config.pass,
        decimals: config.decimals,
        truncate: config.rounding == Rounding::Truncate,
        round_every_level: config.round_every_level,
        by_pass: config.conversion == Conversion::ByPass,
    }
}

fn to_config(s: ScaleSettings) -> GradeConfig {
    GradeConfig {
        min: s.min,
        max: s.max,
        pass: s.pass,
        decimals: s.decimals,
        rounding: if s.truncate {
            Rounding::Truncate
        } else {
            Rounding::Round
        },
        round_every_level: s.round_every_level,
        conversion: if s.by_pass {
            Conversion::ByPass
        } else {
            Conversion::Linear
        },
    }
}

fn to_rules(r: &RulesSettings) -> Result<Rules> {
    Ok(Rules {
        drop_lowest: r.drop_lowest,
        replaces: r
            .replaces
            .as_ref()
            .map(|rep| -> Result<Replacement> {
                Ok(Replacement {
                    target: match &rep.target {
                        None => ReplaceTarget::Lowest,
                        Some(t) => ReplaceTarget::Node(parse_id(t)?),
                    },
                    only_if_better: rep.only_if_better,
                    counts_itself: rep.counts_itself,
                })
            })
            .transpose()?,
        exemption: r.exemption.map(|e| Exemption {
            threshold: e.threshold,
            keep_if_better: e.keep_if_better,
        }),
        minimum: r.minimum.map(|m| Minimum {
            grade: m.grade,
            cap: m.cap,
        }),
        factor: r.factor.map(|f| Factor {
            min: f.min,
            max: f.max,
            neutral: f.neutral,
        }),
    })
}

fn parse_id(id: &str) -> Result<NodeId> {
    id.parse().map_err(|_| format!("id inválido: {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_a_course_and_navigate() {
        let mut data = Data::in_memory().unwrap();
        let home = data.screen(None).unwrap();
        assert!(home.path.is_empty() && home.current.is_none() && home.children.is_empty());
        assert!(home.details.is_none());

        let course = data.create(None, "Cálculo").unwrap();
        let c1 = data.create(Some(&course), "C1").unwrap();
        let c2 = data.create(Some(&course), "C2").unwrap();
        data.set_grade(&c1, Some(4.0)).unwrap();
        data.set_grade(&c2, Some(5.5)).unwrap();

        let home = data.screen(None).unwrap();
        assert_eq!(home.children.len(), 1);
        assert_eq!(home.children[0].display, "4,8");
        assert!(!home.children[0].is_leaf);

        let screen = data.screen(Some(&course)).unwrap();
        assert_eq!(screen.path.len(), 1);
        assert_eq!(screen.path[0].name, "Cálculo");
        let current = screen.current.unwrap();
        assert_eq!(current.display, "4,8");
        assert!(current.complete);
        assert_eq!(current.passed, Some(true));
        let names: Vec<&str> = screen.children.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["C1", "C2"]);
        assert_eq!(screen.children[0].grade, Some(4.0));
        assert_eq!(screen.children[0].display, "4,0");

        let details = screen.details.unwrap();
        assert_eq!(details.mode, Mode::Simple);
        assert!(!details.config.own);
        assert_eq!(details.config.inherited_from, None);
        assert_eq!(details.config.settings, ScaleSettings::SCALE_1_7);
    }

    #[test]
    fn deleted_view_falls_back_to_parent_and_undo_works() {
        let mut data = Data::in_memory().unwrap();
        let course = data.create(None, "Física").unwrap();
        let unit = data.create(Some(&course), "Unidad 1").unwrap();
        data.delete(&unit).unwrap();

        let screen = data.screen(Some(&unit)).unwrap();
        assert_eq!(screen.current.unwrap().id, course);
        assert!(screen.children.is_empty());

        data.restore(&unit).unwrap();
        assert_eq!(data.screen(Some(&course)).unwrap().children.len(), 1);
    }

    #[test]
    fn errors_are_messages() {
        let mut data = Data::in_memory().unwrap();
        let course = data.create(None, "Química").unwrap();
        data.create(Some(&course), "C1").unwrap();
        assert!(data.create(None, "   ").is_err());
        assert!(
            data.set_grade(&course, Some(5.0))
                .unwrap_err()
                .contains("hijos")
        );
        assert!(data.rename("no-es-un-id", "x").is_err());
        let bad = ScaleSettings {
            min: 7.0,
            max: 1.0,
            ..ScaleSettings::SCALE_1_7
        };
        assert!(data.update(&course, Change::Config(Some(bad))).is_err());
    }

    #[test]
    fn changes_survive_reopening() {
        let dir = std::env::temp_dir().join(format!("notable-app-test-{}", NodeId::new()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notable.db");
        let course;
        {
            let mut data = Data::open(&path).unwrap();
            course = data.create(None, "Historia").unwrap();
            let c1 = data.create(Some(&course), "C1").unwrap();
            data.set_grade(&c1, Some(6.2)).unwrap();
            data.rename(&course, "Historia de Chile").unwrap();
            data.update(&course, Change::Target(Some(5.0))).unwrap();
        }
        let data = Data::open(&path).unwrap();
        let screen = data.screen(Some(&course)).unwrap();
        let current = screen.current.unwrap();
        assert_eq!(current.name, "Historia de Chile");
        assert_eq!(current.display, "6,2");
        assert_eq!(screen.details.unwrap().target, Some(5.0));
        drop(data);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn weighting_starts_with_equal_shares() {
        let mut data = Data::in_memory().unwrap();
        let course = data.create(None, "Ramo").unwrap();
        for name in ["A", "B", "C"] {
            data.create(Some(&course), name).unwrap();
        }
        data.update(&course, Change::Mode(Mode::Weighted)).unwrap();
        let screen = data.screen(Some(&course)).unwrap();
        let weights: Vec<f64> = screen.children.iter().map(|c| c.weight.unwrap()).collect();
        assert_eq!(weights, [33.33, 33.33, 33.34]);
        assert!(screen.details.unwrap().warnings.is_empty());

        // Volver a ponderar no pisa los pesos que ya hay.
        let a = screen.children[0].id.clone();
        data.update(&a, Change::Weight(Some(50.0))).unwrap();
        data.update(&course, Change::Mode(Mode::Simple)).unwrap();
        data.update(&course, Change::Mode(Mode::Weighted)).unwrap();
        let screen = data.screen(Some(&course)).unwrap();
        assert_eq!(screen.children[0].weight, Some(50.0));
        assert_eq!(
            screen.details.unwrap().warnings,
            ["Los pesos suman 116,67 %, no 100 %."]
        );
    }

    #[test]
    fn weight_relative_to_ancestor_shows_note() {
        let mut data = Data::in_memory().unwrap();
        let course = data.create(None, "Cálculo").unwrap();
        let exams = data.create(Some(&course), "Certámenes").unwrap();
        let homework = data.create(Some(&course), "Tareas").unwrap();
        data.update(&course, Change::Mode(Mode::Weighted)).unwrap();
        data.update(&exams, Change::Weight(Some(60.0))).unwrap();
        data.update(&homework, Change::Weight(Some(40.0))).unwrap();
        let t1 = data.create(Some(&homework), "T1").unwrap();
        let t2 = data.create(Some(&homework), "T2").unwrap();
        data.update(&homework, Change::Mode(Mode::Weighted))
            .unwrap();
        for (t, w) in [(&t1, 10.0), (&t2, 30.0)] {
            data.update(t, Change::Weight(Some(w))).unwrap();
            data.update(t, Change::WeightBase(Some(course.clone())))
                .unwrap();
        }
        let screen = data.screen(Some(&homework)).unwrap();
        assert_eq!(
            screen.children[0].weight_note.as_deref(),
            Some("= 25 % de Tareas")
        );
        assert_eq!(
            screen.children[0].weight_base.as_deref(),
            Some(course.as_str())
        );
        assert!(screen.details.unwrap().warnings.is_empty());
    }

    #[test]
    fn goal_card() {
        let mut data = Data::in_memory().unwrap();
        let course = data.create(None, "Álgebra").unwrap();
        let c1 = data.create(Some(&course), "Certamen 1").unwrap();
        data.create(Some(&course), "Certamen 2").unwrap();
        data.set_grade(&c1, Some(3.6)).unwrap();
        data.update(&course, Change::Target(Some(4.0))).unwrap();

        let goal = data
            .screen(Some(&course))
            .unwrap()
            .details
            .unwrap()
            .goal
            .unwrap();
        assert_eq!(goal.kind, GoalKind::Needed);
        // El ramo aproxima su nota final: (3,6 + 4,3) / 2 = 3,95 → 4,0.
        assert_eq!(goal.title, "Necesitas 4,3");
        assert_eq!(goal.detail, "en Certamen 2");

        data.update(&course, Change::Target(Some(7.0))).unwrap();
        let goal = data
            .screen(Some(&course))
            .unwrap()
            .details
            .unwrap()
            .goal
            .unwrap();
        assert_eq!(goal.kind, GoalKind::Unreachable);

        data.update(&course, Change::Target(None)).unwrap();
        assert!(
            data.screen(Some(&course))
                .unwrap()
                .details
                .unwrap()
                .goal
                .is_none()
        );
    }

    #[test]
    fn own_and_inherited_config() {
        let mut data = Data::in_memory().unwrap();
        let career = data.create(None, "Ingeniería").unwrap();
        let course = data.create(Some(&career), "MAT021").unwrap();
        data.update(&career, Change::Config(Some(ScaleSettings::SCALE_0_100)))
            .unwrap();
        let details = data.screen(Some(&course)).unwrap().details.unwrap();
        assert!(!details.config.own);
        assert_eq!(details.config.inherited_from.as_deref(), Some("Ingeniería"));
        assert_eq!(details.config.settings, ScaleSettings::SCALE_0_100);

        let mine = ScaleSettings {
            truncate: true,
            ..ScaleSettings::SCALE_1_7
        };
        data.update(&course, Change::Config(Some(mine))).unwrap();
        let details = data.screen(Some(&course)).unwrap().details.unwrap();
        assert!(details.config.own);
        assert_eq!(details.config.settings, mine);

        data.update(&course, Change::Config(None)).unwrap();
        let details = data.screen(Some(&course)).unwrap().details.unwrap();
        assert_eq!(details.config.settings, ScaleSettings::SCALE_0_100);
    }

    #[test]
    fn rules_show_effects_and_badges() {
        let mut data = Data::in_memory().unwrap();
        let course = data.create(None, "Física").unwrap();
        let c1 = data.create(Some(&course), "C1").unwrap();
        let c2 = data.create(Some(&course), "C2").unwrap();
        let exam = data.create(Some(&course), "Examen").unwrap();
        data.set_grade(&c1, Some(3.0)).unwrap();
        data.set_grade(&c2, Some(5.0)).unwrap();
        data.set_grade(&exam, Some(6.0)).unwrap();
        let rules = RulesSettings {
            replaces: Some(ReplaceSettings::default()),
            ..RulesSettings::default()
        };
        data.update(&exam, Change::Rules(rules)).unwrap();

        let screen = data.screen(Some(&course)).unwrap();
        assert_eq!(screen.current.unwrap().display, "5,5");
        assert_eq!(
            screen.children[0].effect.as_deref(),
            Some("Reemplazada por Examen")
        );
        assert_eq!(screen.children[2].badges, ["Reemplaza a la peor"]);
        let details = data.screen(Some(&exam)).unwrap().details.unwrap();
        assert_eq!(details.rules.replaces, Some(ReplaceSettings::default()));
        assert_eq!(details.siblings.len(), 2);

        // Mínimo en C2 no cumplido: el ramo queda reprobado y C2 muestra el aviso.
        let minimum = RulesSettings {
            minimum: Some(MinimumSettings {
                grade: 5.5,
                cap: None,
            }),
            ..RulesSettings::default()
        };
        data.update(&c2, Change::Rules(minimum)).unwrap();
        let screen = data.screen(Some(&course)).unwrap();
        assert_eq!(screen.current.unwrap().passed, Some(false));
        assert_eq!(screen.children[1].badges, ["Mínimo 5,5"]);
        assert_eq!(screen.children[1].warnings.len(), 1);
    }

    #[test]
    fn shift_reorders_siblings() {
        let mut data = Data::in_memory().unwrap();
        let course = data.create(None, "Ramo").unwrap();
        let a = data.create(Some(&course), "A").unwrap();
        data.create(Some(&course), "B").unwrap();
        data.shift(&a, 1).unwrap();
        data.shift(&a, 1).unwrap(); // ya es el último: no pasa nada
        let names: Vec<String> = data
            .screen(Some(&course))
            .unwrap()
            .children
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, ["B", "A"]);
    }

    #[test]
    fn backup_export_and_import_merge() {
        let mut pc = Data::in_memory().unwrap();
        let course = pc.create(None, "Cálculo").unwrap();
        let c1 = pc.create(Some(&course), "C1").unwrap();
        pc.set_grade(&c1, Some(5.5)).unwrap();
        let json = pc.export_backup().unwrap();

        // Otro dispositivo con datos propios: importar no borra nada.
        let mut other = Data::in_memory().unwrap();
        other.create(None, "Historia").unwrap();
        assert_eq!(other.import_backup(&json).unwrap(), 2);
        let names: Vec<String> = other
            .screen(None)
            .unwrap()
            .children
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"Cálculo".to_owned()) && names.contains(&"Historia".to_owned()));
        assert_eq!(
            other
                .screen(Some(&course))
                .unwrap()
                .current
                .unwrap()
                .display,
            "5,5"
        );

        // Un archivo que no es un respaldo no cambia nada.
        assert!(other.import_backup("{\"hola\": 1}").is_err());
        assert_eq!(other.screen(None).unwrap().children.len(), 2);
    }

    #[test]
    fn account_steps_without_network() {
        let mut data = Data::in_memory().unwrap();
        // Usado sin cuenta: nada que sincronizar, y los cambios quedan pendientes.
        assert!(data.sync_begin().unwrap().is_none());
        data.create(None, "Ramo").unwrap();
        assert_eq!(data.pending_count(), 1);

        let session = Session {
            access_token: "a".into(),
            refresh_token: "r".into(),
            expires_at: 0,
            user_id: "u1".into(),
            email: "yo@example.com".into(),
        };
        data.signed_in(&session).unwrap();
        let (stored, since) = data.sync_begin().unwrap().unwrap();
        assert_eq!(stored, session);
        assert_eq!(since, None, "primera vez: se baja todo");

        // Bajó nada, se sube lo pendiente y queda al día.
        let (pending, applied) = data.sync_apply(&[]).unwrap();
        assert_eq!((pending.len(), applied), (1, 0));
        data.sync_end(&pending, 123).unwrap();
        assert_eq!(data.pending_count(), 0);
        assert_eq!(data.last_sync(), Some(123));

        // Cerrar sesión no borra las notas.
        data.store_session(None).unwrap();
        assert!(data.session().unwrap().is_none());
        assert_eq!(data.screen(None).unwrap().children.len(), 1);
    }

    #[test]
    fn move_a_section_somewhere_else() {
        let mut data = Data::in_memory().unwrap();
        let course = data.create(None, "Física").unwrap();
        let lab = data.create(Some(&course), "Laboratorio").unwrap();
        let l1 = data.create(Some(&lab), "L1").unwrap();
        data.set_grade(&l1, Some(6.0)).unwrap();
        let other = data.create(None, "Química").unwrap();

        // Destinos: inicio y todo lo visible, salvo el propio nodo y lo de adentro.
        let dest = data.destinations(&lab).unwrap();
        let names: Vec<(&str, usize, bool)> = dest
            .iter()
            .map(|d| (d.name.as_str(), d.depth, d.current))
            .collect();
        assert_eq!(
            names,
            [
                ("Inicio", 0, false),
                ("Física", 1, true),
                ("Química", 1, false)
            ]
        );

        // Mover con todo lo de adentro (y sus notas).
        data.move_to(&lab, Some(&other)).unwrap();
        let screen = data.screen(Some(&other)).unwrap();
        assert_eq!(screen.children.len(), 1);
        assert_eq!(screen.children[0].display, "6,0");
        assert!(data.screen(Some(&course)).unwrap().children.is_empty());
        assert_eq!(data.screen(Some(&lab)).unwrap().path[0].name, "Química");

        // Sacarlo al inicio y no dejar meterlo dentro de sí mismo.
        data.move_to(&lab, None).unwrap();
        assert_eq!(data.screen(None).unwrap().children.len(), 3);
        assert!(data.move_to(&lab, Some(&l1)).is_err());

        // Una evaluación con nota se marca: si se mete algo adentro, su nota deja de contar.
        let dest = data.destinations(&course).unwrap();
        assert!(dest.iter().any(|d| d.name == "L1" && d.has_grade));
    }

    #[test]
    fn lab_factor_multiplies_the_course() {
        let mut data = Data::in_memory().unwrap();
        let course = data.create(None, "Física").unwrap();
        let c1 = data.create(Some(&course), "C1").unwrap();
        let lab = data.create(Some(&course), "Laboratorio").unwrap();
        data.set_grade(&c1, Some(5.0)).unwrap();
        data.set_grade(&lab, Some(7.0)).unwrap();
        let rules = RulesSettings {
            factor: Some(FactorSettings {
                min: 0.8,
                max: 1.1,
                neutral: None,
            }),
            ..RulesSettings::default()
        };
        data.update(&lab, Change::Rules(rules.clone())).unwrap();

        let screen = data.screen(Some(&course)).unwrap();
        assert_eq!(screen.current.unwrap().display, "5,5");
        let lab_item = &screen.children[1];
        assert_eq!(
            lab_item.effect.as_deref(),
            Some("Multiplica la nota por 1,1")
        );
        assert_eq!(lab_item.badges, ["Factor 0,8 a 1,1"]);
        let details = data.screen(Some(&lab)).unwrap().details.unwrap();
        assert_eq!(details.rules, rules);

        // Al ponderar, el 100 % se reparte solo entre lo que se promedia.
        data.update(&course, Change::Mode(Mode::Weighted)).unwrap();
        let screen = data.screen(Some(&course)).unwrap();
        assert_eq!(screen.children[0].weight, Some(100.0));
        assert_eq!(screen.children[1].weight, None);
        assert!(screen.details.unwrap().warnings.is_empty());

        // Un factor imposible se rechaza.
        let bad = RulesSettings {
            factor: Some(FactorSettings {
                min: 1.2,
                max: 0.8,
                neutral: None,
            }),
            ..RulesSettings::default()
        };
        assert!(data.update(&lab, Change::Rules(bad)).is_err());
    }
}
