use std::collections::{BTreeSet, HashMap};

use crate::{Aggregation, GradeConfig, Node, NodeId, ReplaceTarget, Replacement, Rules, TreeError};

/// El árbol completo en memoria.
///
/// Admite varios nodos de nivel superior (por ejemplo, dos carreras). Los nodos borrados se
/// conservan (borrado lógico) para poder sincronizar el borrado y restaurarlos; las consultas
/// de navegación ([`Tree::children`], [`Tree::roots`]) los omiten, y también omiten todo lo que
/// cuelga de un nodo borrado.
///
/// Cada operación que modifica un nodo lo marca como cambiado; [`Tree::take_dirty`] entrega esos
/// ids para que la capa de guardado los escriba.
#[derive(Clone, Debug, Default)]
pub struct Tree {
    nodes: HashMap<NodeId, Node>,
    /// Hijos de cada padre (`None` = nivel superior), en orden, incluidos los borrados.
    children: HashMap<Option<NodeId>, Vec<NodeId>>,
    dirty: BTreeSet<NodeId>,
}

impl Tree {
    /// Árbol vacío.
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconstruye el árbol a partir de nodos guardados (en cualquier orden).
    ///
    /// Valida que no haya ids repetidos, padres inexistentes, ciclos ni configuraciones
    /// inválidas. Los nodos cargados no quedan marcados como cambiados.
    pub fn from_nodes(nodes: impl IntoIterator<Item = Node>) -> Result<Self, TreeError> {
        let mut map = HashMap::new();
        for node in nodes {
            if let Some(config) = &node.config {
                config.validate()?;
            }
            let id = node.id;
            if map.insert(id, node).is_some() {
                return Err(TreeError::DuplicateId(id));
            }
        }

        for node in map.values() {
            if let Some(parent) = node.parent
                && !map.contains_key(&parent)
            {
                return Err(TreeError::MissingParent {
                    node: node.id,
                    parent,
                });
            }
        }

        // Sin ciclos: subiendo por los padres desde cualquier nodo se llega a la raíz en a lo
        // más `map.len()` pasos.
        for node in map.values() {
            let mut current = node.parent;
            let mut steps = 0;
            while let Some(p) = current {
                steps += 1;
                if p == node.id || steps > map.len() {
                    return Err(TreeError::Cycle(node.id));
                }
                current = map[&p].parent;
            }
        }

        let mut children: HashMap<Option<NodeId>, Vec<NodeId>> = HashMap::new();
        for node in map.values() {
            children.entry(node.parent).or_default().push(node.id);
        }
        for list in children.values_mut() {
            list.sort_by_key(|id| (map[id].position, *id));
        }

        Ok(Self {
            nodes: map,
            children,
            dirty: BTreeSet::new(),
        })
    }

    // ---------------------------------------------------------------- consultas

    /// Un nodo cualquiera, incluso si está borrado.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Todos los nodos, incluidos los borrados, sin orden particular (para guardar).
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    /// Cantidad total de nodos, incluidos los borrados.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Hijos visibles (no borrados) de `parent`, en orden. `None` = nivel superior.
    pub fn children(&self, parent: Option<NodeId>) -> impl Iterator<Item = &Node> {
        self.children
            .get(&parent)
            .into_iter()
            .flatten()
            .map(|id| &self.nodes[id])
            .filter(|n| !n.deleted)
    }

    /// Nodos visibles del nivel superior, en orden.
    pub fn roots(&self) -> impl Iterator<Item = &Node> {
        self.children(None)
    }

    /// Ids de los ancestros de `id`, del padre hacia arriba.
    pub fn ancestors(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let start = self.nodes.get(&id).and_then(|n| n.parent);
        std::iter::successors(start, |p| self.nodes.get(p).and_then(|n| n.parent))
    }

    /// `true` si el nodo existe y ni él ni sus ancestros están borrados.
    pub fn is_alive(&self, id: NodeId) -> bool {
        match self.nodes.get(&id) {
            Some(node) if !node.deleted => self.ancestors(id).all(|a| !self.nodes[&a].deleted),
            _ => false,
        }
    }

    /// `true` si el nodo no tiene hijos visibles (su nota se ingresa, no se calcula).
    pub fn is_leaf(&self, id: NodeId) -> bool {
        self.children(Some(id)).next().is_none()
    }

    /// Configuración de nota que usa `id`: la propia o la del ancestro más cercano que la
    /// defina; si ninguno la define, [`GradeConfig::default`].
    pub fn effective_config(&self, id: NodeId) -> Result<GradeConfig, TreeError> {
        Ok(match self.config_source(id)? {
            Some(source) => self.nodes[&source].config.expect("source has config"),
            None => GradeConfig::default(),
        })
    }

    /// Nodo del que viene la configuración efectiva de `id` (él mismo o un ancestro), o `None`
    /// si se usa la configuración por defecto. Sirve para mostrar "heredado de …".
    pub fn config_source(&self, id: NodeId) -> Result<Option<NodeId>, TreeError> {
        self.node(id)?;
        Ok(std::iter::once(id)
            .chain(self.ancestors(id))
            .find(|n| self.nodes[n].config.is_some()))
    }

    // ---------------------------------------------------------------- cambios

    /// Crea un nodo al final de los hijos de `parent` (`None` = nivel superior).
    pub fn create(
        &mut self,
        parent: Option<NodeId>,
        name: impl Into<String>,
    ) -> Result<NodeId, TreeError> {
        let name = clean_name(name.into())?;
        if let Some(p) = parent {
            self.alive(p)?;
        }
        let mut node = Node::new(name, parent);
        let siblings = self.children.entry(parent).or_default();
        node.position = siblings
            .iter()
            .map(|id| self.nodes[id].position)
            .max()
            .map_or(0, |max| max + 1);
        let id = node.id;
        siblings.push(id);
        self.nodes.insert(id, node);
        self.dirty.insert(id);
        Ok(id)
    }

    /// Cambia el nombre de un nodo.
    pub fn rename(&mut self, id: NodeId, name: impl Into<String>) -> Result<(), TreeError> {
        let name = clean_name(name.into())?;
        self.alive(id)?;
        self.modify(id, |n| n.name = name);
        Ok(())
    }

    /// Mueve `id` para que quede como hijo de `new_parent` (`None` = nivel superior), en la
    /// posición `index` entre los hijos visibles (si `index` es mayor que la cantidad, al final).
    /// También sirve para reordenar dentro del mismo padre.
    pub fn move_node(
        &mut self,
        id: NodeId,
        new_parent: Option<NodeId>,
        index: usize,
    ) -> Result<(), TreeError> {
        self.alive(id)?;
        if let Some(p) = new_parent {
            self.alive(p)?;
            if p == id || self.ancestors(p).any(|a| a == id) {
                return Err(TreeError::Cycle(id));
            }
        }

        let old_parent = self.nodes[&id].parent;
        if let Some(list) = self.children.get_mut(&old_parent) {
            list.retain(|c| *c != id);
        }

        let list = self.children.entry(new_parent).or_default();
        // `index` cuenta solo hermanos visibles: se inserta antes del visible número `index`.
        let at = list
            .iter()
            .enumerate()
            .filter(|(_, c)| !self.nodes[c].deleted)
            .nth(index)
            .map_or(list.len(), |(i, _)| i);
        list.insert(at, id);

        let order = list.clone();
        if old_parent != new_parent {
            self.modify(id, |n| n.parent = new_parent);
        }
        for (position, child) in order.into_iter().enumerate() {
            let position = position as u32;
            if self.nodes[&child].position != position {
                self.modify(child, |n| n.position = position);
            }
        }
        Ok(())
    }

    /// Borra un nodo (borrado lógico). Lo que cuelga de él deja de verse, pero no se modifica,
    /// así [`Tree::restore`] lo recupera todo.
    pub fn delete(&mut self, id: NodeId) -> Result<(), TreeError> {
        self.alive(id)?;
        self.modify(id, |n| n.deleted = true);
        Ok(())
    }

    /// Recupera un nodo borrado. Su padre debe estar vivo.
    pub fn restore(&mut self, id: NodeId) -> Result<(), TreeError> {
        let node = self.node(id)?;
        if let Some(p) = node.parent {
            self.alive(p)?;
        }
        if node.deleted {
            self.modify(id, |n| n.deleted = false);
        }
        Ok(())
    }

    /// Ingresa o borra (`None`) la nota de una hoja.
    pub fn set_grade(&mut self, id: NodeId, grade: Option<f64>) -> Result<(), TreeError> {
        self.alive(id)?;
        if grade.is_some_and(|g| !g.is_finite()) {
            return Err(TreeError::InvalidNumber);
        }
        if !self.is_leaf(id) {
            return Err(TreeError::NotALeaf(id));
        }
        self.modify(id, |n| n.grade = grade);
        Ok(())
    }

    /// Asigna o quita (`None`) el peso de un nodo dentro de su padre. Debe ser ≥ 0.
    pub fn set_weight(&mut self, id: NodeId, weight: Option<f64>) -> Result<(), TreeError> {
        self.alive(id)?;
        if weight.is_some_and(|w| !w.is_finite() || w < 0.0) {
            return Err(TreeError::InvalidNumber);
        }
        self.modify(id, |n| n.weight = weight);
        Ok(())
    }

    /// Cambia cómo se combinan los hijos del nodo (simple o ponderado).
    pub fn set_aggregation(
        &mut self,
        id: NodeId,
        aggregation: Aggregation,
    ) -> Result<(), TreeError> {
        self.alive(id)?;
        self.modify(id, |n| n.aggregation = aggregation);
        Ok(())
    }

    /// Define (o quita con `None`, para heredar) la configuración de nota del nodo.
    pub fn set_config(&mut self, id: NodeId, config: Option<GradeConfig>) -> Result<(), TreeError> {
        self.alive(id)?;
        if let Some(c) = &config {
            c.validate()?;
        }
        self.modify(id, |n| n.config = config);
        Ok(())
    }

    /// Indica respecto a qué ancestro se expresa el peso del nodo ("vale 20 % del ramo").
    /// `None`, o el propio padre, significa respecto al padre.
    pub fn set_weight_base(&mut self, id: NodeId, base: Option<NodeId>) -> Result<(), TreeError> {
        self.alive(id)?;
        let parent = self.nodes[&id].parent;
        let base = match base {
            Some(b) if Some(b) == parent => None,
            Some(b) if self.ancestors(id).any(|a| a == b) => Some(b),
            Some(b) => return Err(TreeError::NotAnAncestor { node: id, base: b }),
            None => None,
        };
        self.modify(id, |n| n.weight_base = base);
        Ok(())
    }

    /// Fuerza (`Some`) o deja según la configuración (`None`) el redondeo de la nota calculada
    /// de este nodo.
    pub fn set_round_result(&mut self, id: NodeId, round: Option<bool>) -> Result<(), TreeError> {
        self.alive(id)?;
        self.modify(id, |n| n.round_result = round);
        Ok(())
    }

    /// Define o quita (`None`) la nota objetivo del nodo, en su propia escala. Puede quedar fuera
    /// de la escala (el cálculo lo avisa y el objetivo resulta inalcanzable).
    pub fn set_target(&mut self, id: NodeId, target: Option<f64>) -> Result<(), TreeError> {
        self.alive(id)?;
        if target.is_some_and(|t| !t.is_finite()) {
            return Err(TreeError::InvalidNumber);
        }
        self.modify(id, |n| n.target = target);
        Ok(())
    }

    /// Asigna o quita (`None`) los créditos del nodo. Deben ser ≥ 0.
    pub fn set_credits(&mut self, id: NodeId, credits: Option<f64>) -> Result<(), TreeError> {
        self.alive(id)?;
        if credits.is_some_and(|c| !c.is_finite() || c < 0.0) {
            return Err(TreeError::InvalidNumber);
        }
        self.modify(id, |n| n.credits = credits);
        Ok(())
    }

    /// Define las reglas opcionales del nodo (`Rules::default()` las apaga todas).
    pub fn set_rules(&mut self, id: NodeId, rules: Rules) -> Result<(), TreeError> {
        self.alive(id)?;
        let finite = |x: f64| x.is_finite();
        if rules.exemption.is_some_and(|e| !finite(e.threshold))
            || rules
                .minimum
                .is_some_and(|m| !finite(m.grade) || m.cap.is_some_and(|c| !finite(c)))
        {
            return Err(TreeError::InvalidRule);
        }
        if rules.replaces.is_some() && rules.exemption.is_some() {
            return Err(TreeError::InvalidRule);
        }
        if let Some(Replacement {
            target: ReplaceTarget::Node(target),
            ..
        }) = rules.replaces
        {
            let parent = self.nodes[&id].parent;
            let sibling = target != id
                && self.is_alive(target)
                && self.nodes.get(&target).is_some_and(|t| t.parent == parent);
            if !sibling {
                return Err(TreeError::InvalidRule);
            }
        }
        self.modify(id, |n| n.rules = rules);
        Ok(())
    }

    /// Créditos del nodo: los propios o, si no tiene, la suma de los de sus hijos visibles
    /// (`None` si nadie debajo tiene créditos).
    pub fn effective_credits(&self, id: NodeId) -> Option<f64> {
        let node = self.nodes.get(&id)?;
        if node.credits.is_some() {
            return node.credits;
        }
        self.children(Some(id))
            .filter_map(|c| self.effective_credits(c.id))
            .fold(None, |acc, c| Some(acc.unwrap_or(0.0) + c))
    }

    // ---------------------------------------------------------------- cambios pendientes

    /// Entrega (y olvida) los ids de los nodos modificados desde la última llamada.
    pub fn take_dirty(&mut self) -> Vec<NodeId> {
        std::mem::take(&mut self.dirty).into_iter().collect()
    }

    /// `true` si el nodo cambió desde el último [`Tree::take_dirty`].
    pub fn is_dirty(&self, id: NodeId) -> bool {
        self.dirty.contains(&id)
    }

    /// Vuelve a marcar nodos como cambiados (por ejemplo, si guardarlos falló después de
    /// [`Tree::take_dirty`]). Ignora los ids que no existen.
    pub fn mark_dirty(&mut self, ids: impl IntoIterator<Item = NodeId>) {
        for id in ids {
            if self.nodes.contains_key(&id) {
                self.dirty.insert(id);
            }
        }
    }

    // ---------------------------------------------------------------- internos

    fn node(&self, id: NodeId) -> Result<&Node, TreeError> {
        self.nodes.get(&id).ok_or(TreeError::NotFound(id))
    }

    pub(crate) fn alive(&self, id: NodeId) -> Result<(), TreeError> {
        self.node(id)?;
        if self.is_alive(id) {
            Ok(())
        } else {
            Err(TreeError::Deleted(id))
        }
    }

    fn modify(&mut self, id: NodeId, f: impl FnOnce(&mut Node)) {
        f(self.nodes.get_mut(&id).expect("checked by caller"));
        self.dirty.insert(id);
    }
}

fn clean_name(name: String) -> Result<String, TreeError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        Err(TreeError::EmptyName)
    } else {
        Ok(trimmed.to_owned())
    }
}
