//! Estado de la app: el árbol en memoria y la base local. Cada cambio se guarda al instante.
//!
//! No depende de Tauri, así se prueba directo.

use std::path::Path;

use notable_api::{Crumb, Item, Scale, Screen, format_grade};
use notable_core::{Evaluation, NodeId, Tree};
use notable_store::Store;

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
                ids.into_iter()
                    .map(|id| Crumb {
                        id: id.to_string(),
                        name: self.tree.get(id).expect("ancestro").name.clone(),
                    })
                    .collect()
            }
            None => Vec::new(),
        };
        Ok(Screen {
            path,
            current: current.map(|id| self.item(&eval, id)),
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

    fn save(&mut self) -> Result<()> {
        self.store
            .save(&mut self.tree)
            .map(|_| ())
            .map_err(|e| format!("no se pudo guardar: {e}"))
    }

    fn item(&self, eval: &Evaluation, id: NodeId) -> Item {
        let node = self.tree.get(id).expect("nodo visible");
        let result = eval.get(id).expect("nodo visible");
        let config = result.config;
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
        }
    }
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
        }
        let data = Data::open(&path).unwrap();
        let screen = data.screen(Some(&course)).unwrap();
        let current = screen.current.unwrap();
        assert_eq!(current.name, "Historia de Chile");
        assert_eq!(current.display, "6,2");
        drop(data);
        let _ = std::fs::remove_dir_all(dir);
    }
}
