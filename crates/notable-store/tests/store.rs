//! Pruebas de la Fase 5: guardado local.

use notable_core::{Aggregation, GradeConfig, Minimum, Node, NodeId, Rules, Tree};
use notable_store::{SCHEMA_VERSION, Store, StoreError};
use tempfile::TempDir;

/// Un árbol con un poco de todo: configuración, pesos, objetivo, créditos, reglas y un nodo
/// borrado.
fn sample_tree() -> (Tree, NodeId) {
    let mut tree = Tree::new();
    let career = tree.create(None, "Ingeniería").unwrap();
    let semester = tree.create(Some(career), "Semestre 1").unwrap();
    tree.set_aggregation(semester, Aggregation::Credits)
        .unwrap();

    let course = tree.create(Some(semester), "Cálculo").unwrap();
    tree.set_credits(course, Some(10.0)).unwrap();
    tree.set_config(course, Some(GradeConfig::SCALE_0_100))
        .unwrap();
    tree.set_aggregation(course, Aggregation::Weighted).unwrap();
    tree.set_target(course, Some(55.0)).unwrap();
    for (name, weight, grade) in [
        ("C1", 30.0, Some(60.0)),
        ("C2", 30.0, Some(48.0)),
        ("Examen", 40.0, None),
    ] {
        let id = tree.create(Some(course), name).unwrap();
        tree.set_weight(id, Some(weight)).unwrap();
        tree.set_grade(id, grade).unwrap();
    }
    let exam = tree.children(Some(course)).last().unwrap().id;
    tree.set_rules(
        exam,
        Rules {
            minimum: Some(Minimum {
                grade: 40.0,
                cap: None,
            }),
            ..Rules::default()
        },
    )
    .unwrap();

    let gone = tree.create(Some(semester), "Ramo borrado").unwrap();
    tree.create(Some(gone), "Nota").unwrap();
    tree.delete(gone).unwrap();
    (tree, course)
}

fn sorted(tree: &Tree) -> Vec<Node> {
    let mut nodes: Vec<Node> = tree.nodes().cloned().collect();
    nodes.sort_by_key(|n| n.id);
    nodes
}

fn db_path(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("notable.db")
}

// ------------------------------------------------------------------ guardar y abrir

#[test]
fn save_close_open_gives_the_same_tree() {
    let dir = TempDir::new().unwrap();
    let (mut tree, course) = sample_tree();
    let total = tree.len();
    {
        let mut store = Store::open(db_path(&dir)).unwrap();
        assert_eq!(store.save(&mut tree).unwrap(), total);
    } // se cierra

    let store = Store::open(db_path(&dir)).unwrap();
    let loaded = store.load_tree().unwrap();
    assert_eq!(sorted(&loaded), sorted(&tree));
    // Lo cargado no queda como cambiado.
    assert!(!loaded.is_dirty(course));
    // Y se calcula igual.
    assert_eq!(
        loaded.evaluate().grade(course),
        tree.evaluate().grade(course)
    );
    assert_eq!(
        loaded.requirement(course).unwrap(),
        tree.requirement(course).unwrap()
    );
}

#[test]
fn each_change_is_saved() {
    let dir = TempDir::new().unwrap();
    let (mut tree, course) = sample_tree();
    let mut store = Store::open(db_path(&dir)).unwrap();
    store.save(&mut tree).unwrap();
    assert_eq!(store.save(&mut tree).unwrap(), 0, "nada nuevo que guardar");

    tree.rename(course, "Cálculo I").unwrap();
    assert_eq!(store.save(&mut tree).unwrap(), 1);
    let exam = tree.children(Some(course)).last().unwrap().id;
    tree.set_grade(exam, Some(70.0)).unwrap();
    store.save(&mut tree).unwrap();
    drop(store);

    let loaded = Store::open(db_path(&dir)).unwrap().load_tree().unwrap();
    assert_eq!(loaded.get(course).unwrap().name, "Cálculo I");
    assert_eq!(loaded.get(exam).unwrap().grade, Some(70.0));
    assert_eq!(sorted(&loaded), sorted(&tree));
}

#[test]
fn saved_rows_are_pending_sync_and_time_moves_forward() {
    let mut store = Store::open_in_memory().unwrap();
    let mut tree = Tree::new();
    let a = tree.create(None, "A").unwrap();
    let b = tree.create(None, "B").unwrap();
    store.save(&mut tree).unwrap();

    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(store.dirty_ids().unwrap(), expected);

    let first = store.updated_at(a).unwrap().unwrap();
    tree.rename(a, "A2").unwrap();
    store.save(&mut tree).unwrap();
    let second = store.updated_at(a).unwrap().unwrap();
    assert!(second > first, "{second} > {first}");
    assert_eq!(store.updated_at(NodeId::new()).unwrap(), None);
}

#[test]
fn empty_database_gives_empty_tree() {
    let store = Store::open_in_memory().unwrap();
    assert!(store.load_tree().unwrap().is_empty());
}

// ------------------------------------------------------------------ migraciones

#[test]
fn migrations_run_once_and_set_the_version() {
    let dir = TempDir::new().unwrap();
    let mut tree = Tree::new();
    tree.create(None, "A").unwrap();
    Store::open(db_path(&dir)).unwrap().save(&mut tree).unwrap();
    // Abrir otra vez no rehace nada ni pierde datos.
    let store = Store::open(db_path(&dir)).unwrap();
    assert_eq!(store.load_tree().unwrap().len(), 1);
    drop(store);

    let conn = rusqlite::Connection::open(db_path(&dir)).unwrap();
    let version: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
}

#[test]
fn newer_database_is_refused() {
    let dir = TempDir::new().unwrap();
    {
        let conn = rusqlite::Connection::open(db_path(&dir)).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
    }
    assert!(matches!(
        Store::open(db_path(&dir)),
        Err(StoreError::NewerSchema { .. })
    ));
}

#[test]
fn corrupt_row_is_reported() {
    let dir = TempDir::new().unwrap();
    Store::open(db_path(&dir)).unwrap();
    {
        let conn = rusqlite::Connection::open(db_path(&dir)).unwrap();
        conn.execute(
            "INSERT INTO items (id, data, updated_at) VALUES ('x', 'no es json', 0)",
            [],
        )
        .unwrap();
    }
    let store = Store::open(db_path(&dir)).unwrap();
    assert!(matches!(store.load_tree(), Err(StoreError::CorruptRow { id, .. }) if id == "x"));
}

// ------------------------------------------------------------------ respaldo

#[test]
fn export_then_import_into_new_store() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("respaldo.json");
    let (mut tree, _) = sample_tree();
    let mut store = Store::open_in_memory().unwrap();
    store.save(&mut tree).unwrap();
    store.export_to_file(&file).unwrap();

    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains(r#""format": "notable""#), "{text}");

    let mut other = Store::open_in_memory().unwrap();
    let imported = other.import_from_file(&file).unwrap();
    assert_eq!(sorted(&imported), sorted(&tree));
    // Y quedó guardado en la otra base, pendiente de sincronizar.
    assert_eq!(sorted(&other.load_tree().unwrap()), sorted(&tree));
    assert_eq!(other.dirty_ids().unwrap().len(), tree.len());
}

#[test]
fn import_merges_without_deleting() {
    // El respaldo trae "A" (con otro nombre) y "B"; aquí existe "A" y "Local".
    let mut store = Store::open_in_memory().unwrap();
    let mut tree = Tree::new();
    let a = tree.create(None, "A").unwrap();
    store.save(&mut tree).unwrap();
    let backup = store.export_json().unwrap();

    tree.rename(a, "A cambiado").unwrap();
    let local = tree.create(None, "Local").unwrap();
    store.save(&mut tree).unwrap();

    let mut source = Store::open_in_memory().unwrap();
    let mut source_tree = source.import_json(&backup).unwrap();
    let b = source_tree.create(None, "B").unwrap();
    source.save(&mut source_tree).unwrap();

    let merged = store.import_json(&source.export_json().unwrap()).unwrap();
    assert_eq!(
        merged.get(a).unwrap().name,
        "A",
        "el respaldo gana en el mismo id"
    );
    assert!(merged.get(local).is_some(), "lo local no se borra");
    assert!(merged.get(b).is_some());
    assert_eq!(sorted(&store.load_tree().unwrap()), sorted(&merged));
}

#[test]
fn invalid_backups_change_nothing() {
    let mut store = Store::open_in_memory().unwrap();
    let mut tree = Tree::new();
    tree.create(None, "A").unwrap();
    store.save(&mut tree).unwrap();
    let before = sorted(&store.load_tree().unwrap());

    let not_notable = r#"{"format":"otro","version":1,"exported_at":0,"nodes":[]}"#;
    let newer = r#"{"format":"notable","version":99,"exported_at":0,"nodes":[]}"#;
    // Un hijo cuyo padre no existe.
    let orphan = Node::new("Huérfano", Some(NodeId::new()));
    let broken = format!(
        r#"{{"format":"notable","version":1,"exported_at":0,"nodes":[{}]}}"#,
        orphan.to_json().unwrap()
    );
    for json in [not_notable, newer, "no es json", broken.as_str()] {
        assert!(store.import_json(json).is_err(), "{json}");
    }
    assert!(matches!(
        store.import_json(&broken),
        Err(StoreError::Tree(_))
    ));
    assert_eq!(sorted(&store.load_tree().unwrap()), before);
}
