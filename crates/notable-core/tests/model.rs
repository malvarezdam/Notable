//! Pruebas de la Fase 1: construcción del árbol, herencia y serialización.

use notable_core::{
    Aggregation, ConfigError, GradeConfig, Node, NodeId, Rounding, Tree, TreeError,
};

/// Carrera › Semestre › Cálculo › {Certamen 1, Certamen 2, Tareas › {T1, T2}}
struct Sample {
    tree: Tree,
    career: NodeId,
    semester: NodeId,
    course: NodeId,
    c1: NodeId,
    c2: NodeId,
    homework: NodeId,
    t1: NodeId,
    t2: NodeId,
}

fn sample() -> Sample {
    let mut tree = Tree::new();
    let career = tree.create(None, "Ingeniería").unwrap();
    let semester = tree.create(Some(career), "Semestre 1").unwrap();
    let course = tree.create(Some(semester), "Cálculo").unwrap();
    let c1 = tree.create(Some(course), "Certamen 1").unwrap();
    let c2 = tree.create(Some(course), "Certamen 2").unwrap();
    let homework = tree.create(Some(course), "Tareas").unwrap();
    let t1 = tree.create(Some(homework), "T1").unwrap();
    let t2 = tree.create(Some(homework), "T2").unwrap();
    Sample {
        tree,
        career,
        semester,
        course,
        c1,
        c2,
        homework,
        t1,
        t2,
    }
}

fn names(tree: &Tree, parent: Option<NodeId>) -> Vec<String> {
    tree.children(parent).map(|n| n.name.clone()).collect()
}

/// Compara dos árboles por su contenido (todos los nodos, incluidos borrados).
fn same_nodes(a: &Tree, b: &Tree) -> bool {
    a.len() == b.len() && a.nodes().all(|n| b.get(n.id) == Some(n))
}

// ------------------------------------------------------------------ construcción

#[test]
fn build_tree_in_order() {
    let s = sample();
    assert_eq!(s.tree.len(), 8);
    assert_eq!(names(&s.tree, None), ["Ingeniería"]);
    assert_eq!(
        names(&s.tree, Some(s.course)),
        ["Certamen 1", "Certamen 2", "Tareas"]
    );
    assert_eq!(
        s.tree.ancestors(s.t1).collect::<Vec<_>>(),
        [s.homework, s.course, s.semester, s.career]
    );
    assert!(s.tree.is_leaf(s.c1));
    assert!(!s.tree.is_leaf(s.course));
}

#[test]
fn new_nodes_use_simple_defaults() {
    let s = sample();
    let node = s.tree.get(s.course).unwrap();
    assert_eq!(node.aggregation, Aggregation::Simple);
    assert_eq!(node.grade, None);
    assert_eq!(node.weight, None);
    assert_eq!(node.config, None);
    assert!(!node.deleted);
}

#[test]
fn several_top_level_nodes() {
    let mut s = sample();
    s.tree.create(None, "Diplomado").unwrap();
    assert_eq!(names(&s.tree, None), ["Ingeniería", "Diplomado"]);
}

#[test]
fn names_are_trimmed_and_not_empty() {
    let mut s = sample();
    assert_eq!(s.tree.create(None, "   "), Err(TreeError::EmptyName));
    s.tree.rename(s.c1, "  Control 1 ").unwrap();
    assert_eq!(s.tree.get(s.c1).unwrap().name, "Control 1");
    assert_eq!(s.tree.rename(s.c1, ""), Err(TreeError::EmptyName));
}

#[test]
fn unknown_parent_is_an_error() {
    let mut tree = Tree::new();
    let ghost = NodeId::new();
    assert_eq!(
        tree.create(Some(ghost), "x"),
        Err(TreeError::NotFound(ghost))
    );
}

// ------------------------------------------------------------------ mover

#[test]
fn reorder_within_parent() {
    let mut s = sample();
    s.tree.move_node(s.homework, Some(s.course), 0).unwrap();
    assert_eq!(
        names(&s.tree, Some(s.course)),
        ["Tareas", "Certamen 1", "Certamen 2"]
    );
    // Índice mayor que la cantidad: al final.
    s.tree.move_node(s.homework, Some(s.course), 99).unwrap();
    assert_eq!(
        names(&s.tree, Some(s.course)),
        ["Certamen 1", "Certamen 2", "Tareas"]
    );
}

#[test]
fn move_to_other_parent() {
    let mut s = sample();
    s.tree.move_node(s.t1, Some(s.course), 1).unwrap();
    assert_eq!(s.tree.get(s.t1).unwrap().parent, Some(s.course));
    assert_eq!(
        names(&s.tree, Some(s.course)),
        ["Certamen 1", "T1", "Certamen 2", "Tareas"]
    );
    assert_eq!(names(&s.tree, Some(s.homework)), ["T2"]);
}

#[test]
fn move_to_top_level() {
    let mut s = sample();
    s.tree.move_node(s.course, None, 0).unwrap();
    assert_eq!(names(&s.tree, None), ["Cálculo", "Ingeniería"]);
    assert_eq!(s.tree.get(s.course).unwrap().parent, None);
}

#[test]
fn move_into_itself_or_descendant_is_rejected() {
    let mut s = sample();
    assert_eq!(
        s.tree.move_node(s.course, Some(s.course), 0),
        Err(TreeError::Cycle(s.course))
    );
    assert_eq!(
        s.tree.move_node(s.course, Some(s.t1), 0),
        Err(TreeError::Cycle(s.course))
    );
    // No cambió nada.
    assert_eq!(s.tree.get(s.course).unwrap().parent, Some(s.semester));
}

#[test]
fn move_index_skips_deleted_siblings() {
    let mut s = sample();
    s.tree.delete(s.c1).unwrap();
    // Visibles: [Certamen 2, Tareas]; índice 1 = antes de Tareas.
    s.tree.move_node(s.t1, Some(s.course), 1).unwrap();
    assert_eq!(
        names(&s.tree, Some(s.course)),
        ["Certamen 2", "T1", "Tareas"]
    );
}

// ------------------------------------------------------------------ borrar

#[test]
fn delete_hides_node_and_descendants() {
    let mut s = sample();
    s.tree.delete(s.homework).unwrap();
    assert_eq!(names(&s.tree, Some(s.course)), ["Certamen 1", "Certamen 2"]);
    assert!(s.tree.get(s.homework).unwrap().deleted);
    // Los hijos no se tocan, pero quedan ocultos.
    assert!(!s.tree.get(s.t1).unwrap().deleted);
    assert!(!s.tree.is_alive(s.t1));
    assert_eq!(
        s.tree.set_grade(s.t1, Some(5.0)),
        Err(TreeError::Deleted(s.t1))
    );
    assert_eq!(
        s.tree.create(Some(s.homework), "T3"),
        Err(TreeError::Deleted(s.homework))
    );
    // El nodo sigue existiendo para sincronizar el borrado.
    assert_eq!(s.tree.len(), 8);
}

#[test]
fn restore_brings_everything_back_in_place() {
    let mut s = sample();
    s.tree.delete(s.homework).unwrap();
    s.tree.restore(s.homework).unwrap();
    assert_eq!(
        names(&s.tree, Some(s.course)),
        ["Certamen 1", "Certamen 2", "Tareas"]
    );
    assert_eq!(names(&s.tree, Some(s.homework)), ["T1", "T2"]);
    assert!(s.tree.is_alive(s.t2));
}

#[test]
fn cannot_restore_under_deleted_parent() {
    let mut s = sample();
    s.tree.delete(s.t1).unwrap();
    s.tree.delete(s.homework).unwrap();
    assert_eq!(s.tree.restore(s.t1), Err(TreeError::Deleted(s.homework)));
}

#[test]
fn leaf_again_after_deleting_its_children() {
    let mut s = sample();
    assert_eq!(
        s.tree.set_grade(s.homework, Some(6.0)),
        Err(TreeError::NotALeaf(s.homework))
    );
    s.tree.delete(s.t1).unwrap();
    s.tree.delete(s.t2).unwrap();
    assert!(s.tree.is_leaf(s.homework));
    s.tree.set_grade(s.homework, Some(6.0)).unwrap();
}

// ------------------------------------------------------------------ notas y pesos

#[test]
fn edit_grade() {
    let mut s = sample();
    s.tree.set_grade(s.c1, Some(5.5)).unwrap();
    assert_eq!(s.tree.get(s.c1).unwrap().grade, Some(5.5));
    s.tree.set_grade(s.c1, None).unwrap();
    assert_eq!(s.tree.get(s.c1).unwrap().grade, None);
}

#[test]
fn grade_out_of_scale_is_allowed_but_nan_is_not() {
    // Fuera de escala es un aviso (Fase 2), no un error.
    let mut s = sample();
    s.tree.set_grade(s.c1, Some(8.0)).unwrap();
    assert_eq!(
        s.tree.set_grade(s.c1, Some(f64::NAN)),
        Err(TreeError::InvalidNumber)
    );
    assert_eq!(s.tree.get(s.c1).unwrap().grade, Some(8.0));
}

#[test]
fn weights_and_aggregation() {
    let mut s = sample();
    s.tree
        .set_aggregation(s.course, Aggregation::Weighted)
        .unwrap();
    s.tree.set_weight(s.c1, Some(30.0)).unwrap();
    assert_eq!(
        s.tree.get(s.course).unwrap().aggregation,
        Aggregation::Weighted
    );
    assert_eq!(s.tree.get(s.c1).unwrap().weight, Some(30.0));
    assert_eq!(
        s.tree.set_weight(s.c1, Some(-1.0)),
        Err(TreeError::InvalidNumber)
    );
    s.tree.set_weight(s.c1, None).unwrap();
    assert_eq!(s.tree.get(s.c1).unwrap().weight, None);
}

// ------------------------------------------------------------------ herencia de configuración

#[test]
fn default_config_when_nobody_defines_one() {
    let s = sample();
    assert_eq!(
        s.tree.effective_config(s.t1).unwrap(),
        GradeConfig::default()
    );
    assert_eq!(s.tree.config_source(s.t1).unwrap(), None);
}

#[test]
fn nearest_ancestor_config_wins() {
    let mut s = sample();
    s.tree
        .set_config(s.career, Some(GradeConfig::SCALE_1_7))
        .unwrap();
    s.tree
        .set_config(s.course, Some(GradeConfig::SCALE_0_100))
        .unwrap();

    assert_eq!(
        s.tree.effective_config(s.semester).unwrap(),
        GradeConfig::SCALE_1_7
    );
    assert_eq!(
        s.tree.effective_config(s.t1).unwrap(),
        GradeConfig::SCALE_0_100
    );
    assert_eq!(s.tree.config_source(s.t1).unwrap(), Some(s.course));
    assert_eq!(s.tree.config_source(s.course).unwrap(), Some(s.course));

    // Quitar la del ramo vuelve a heredar la de la carrera.
    s.tree.set_config(s.course, None).unwrap();
    assert_eq!(
        s.tree.effective_config(s.t1).unwrap(),
        GradeConfig::SCALE_1_7
    );
    assert_eq!(s.tree.config_source(s.t1).unwrap(), Some(s.career));
}

#[test]
fn own_config_overrides_inherited() {
    let mut s = sample();
    s.tree
        .set_config(s.course, Some(GradeConfig::SCALE_0_100))
        .unwrap();
    let custom = GradeConfig::new(1.0, 7.0, 4.0, 2, Rounding::Truncate).unwrap();
    s.tree.set_config(s.homework, Some(custom)).unwrap();
    assert_eq!(s.tree.effective_config(s.t2).unwrap(), custom);
    assert_eq!(
        s.tree.effective_config(s.c1).unwrap(),
        GradeConfig::SCALE_0_100
    );
}

#[test]
fn moving_a_node_changes_what_it_inherits() {
    let mut s = sample();
    s.tree
        .set_config(s.homework, Some(GradeConfig::SCALE_0_100))
        .unwrap();
    assert_eq!(
        s.tree.effective_config(s.t1).unwrap(),
        GradeConfig::SCALE_0_100
    );
    s.tree.move_node(s.t1, Some(s.course), 0).unwrap();
    assert_eq!(
        s.tree.effective_config(s.t1).unwrap(),
        GradeConfig::default()
    );
}

#[test]
fn invalid_configs_are_rejected() {
    assert_eq!(
        GradeConfig::new(7.0, 1.0, 4.0, 1, Rounding::Round),
        Err(ConfigError::EmptyScale)
    );
    assert_eq!(
        GradeConfig::new(1.0, 7.0, 8.0, 1, Rounding::Round),
        Err(ConfigError::PassOutOfScale)
    );
    assert_eq!(
        GradeConfig::new(1.0, 7.0, 4.0, 9, Rounding::Round),
        Err(ConfigError::TooManyDecimals)
    );
    assert_eq!(
        GradeConfig::new(f64::NAN, 7.0, 4.0, 1, Rounding::Round),
        Err(ConfigError::NotFinite)
    );

    let mut s = sample();
    let bad = GradeConfig {
        min: 10.0,
        ..GradeConfig::default()
    };
    assert_eq!(
        s.tree.set_config(s.course, Some(bad)),
        Err(TreeError::Config(ConfigError::EmptyScale))
    );
}

// ------------------------------------------------------------------ cambios pendientes

#[test]
fn dirty_tracks_changed_nodes() {
    let mut s = sample();
    assert_eq!(s.tree.take_dirty().len(), 8); // los recién creados
    assert!(s.tree.take_dirty().is_empty());

    s.tree.set_grade(s.c1, Some(4.0)).unwrap();
    assert!(s.tree.is_dirty(s.c1));
    assert_eq!(s.tree.take_dirty(), [s.c1]);

    // Mover marca el nodo y los hermanos cuya posición cambió.
    s.tree.move_node(s.homework, Some(s.course), 0).unwrap();
    let mut changed = s.tree.take_dirty();
    changed.sort();
    let mut expected = vec![s.homework, s.c1, s.c2];
    expected.sort();
    assert_eq!(changed, expected);

    // Una operación rechazada no marca nada.
    let _ = s.tree.set_grade(s.course, Some(5.0));
    assert!(s.tree.take_dirty().is_empty());
}

// ------------------------------------------------------------------ serialización

#[test]
fn node_json_round_trip() {
    let mut s = sample();
    s.tree.set_grade(s.c1, Some(5.25)).unwrap();
    s.tree.set_weight(s.c1, Some(35.0)).unwrap();
    s.tree
        .set_aggregation(s.course, Aggregation::Weighted)
        .unwrap();
    s.tree
        .set_config(s.course, Some(GradeConfig::SCALE_0_100))
        .unwrap();
    s.tree.delete(s.c2).unwrap();

    for node in s.tree.nodes() {
        let json = node.to_json().unwrap();
        assert_eq!(&Node::from_json(&json).unwrap(), node, "{json}");
    }
}

#[test]
fn tree_round_trip_through_json_rows() {
    let mut s = sample();
    s.tree.set_grade(s.t1, Some(6.5)).unwrap();
    s.tree
        .set_config(s.career, Some(GradeConfig::SCALE_1_7))
        .unwrap();
    s.tree.move_node(s.homework, Some(s.course), 0).unwrap();
    s.tree.delete(s.c2).unwrap();

    // Como lo haría la base de datos: una fila JSON por nodo, en orden cualquiera.
    let mut rows: Vec<String> = s.tree.nodes().map(|n| n.to_json().unwrap()).collect();
    rows.reverse();
    let loaded = Tree::from_nodes(rows.iter().map(|r| Node::from_json(r).unwrap())).unwrap();

    assert!(same_nodes(&s.tree, &loaded));
    assert_eq!(names(&loaded, Some(s.course)), ["Tareas", "Certamen 1"]);
    assert_eq!(
        loaded.effective_config(s.t1).unwrap(),
        GradeConfig::SCALE_1_7
    );
    assert!(loaded.clone().take_dirty().is_empty());
}

#[test]
fn json_format_is_stable() {
    let id: NodeId = "6f1c1c1e-6a4b-4f7e-9a59-1d2a3b4c5d6e".parse().unwrap();
    let node = Node {
        id,
        parent: None,
        name: "Cálculo".into(),
        position: 2,
        grade: None,
        weight: Some(20.0),
        weight_base: None,
        round_result: Some(true),
        target: None,
        aggregation: Aggregation::Weighted,
        config: Some(GradeConfig::SCALE_0_100),
        deleted: false,
    };
    assert_eq!(
        node.to_json().unwrap(),
        r#"{"id":"6f1c1c1e-6a4b-4f7e-9a59-1d2a3b4c5d6e","parent":null,"name":"Cálculo","position":2,"weight":20.0,"round_result":true,"aggregation":"weighted","config":{"min":0.0,"max":100.0,"pass":55.0,"decimals":0,"rounding":"round","round_every_level":false,"conversion":"linear"},"deleted":false}"#
    );
}

#[test]
fn old_and_future_json_still_loads() {
    // Datos mínimos (versión vieja): lo que falta toma su valor por defecto.
    let minimal = r#"{"id":"6f1c1c1e-6a4b-4f7e-9a59-1d2a3b4c5d6e","name":"Ramo"}"#;
    let node = Node::from_json(minimal).unwrap();
    assert_eq!(node.parent, None);
    assert_eq!(node.aggregation, Aggregation::Simple);
    assert!(!node.deleted);

    // Campos desconocidos (versión más nueva): se ignoran.
    let future = r#"{"id":"6f1c1c1e-6a4b-4f7e-9a59-1d2a3b4c5d6e","name":"Ramo","credits":6,"config":{"min":0,"max":10,"decimals":2}}"#;
    let node = Node::from_json(future).unwrap();
    // Configuración parcial: completa con los valores por defecto.
    let config = node.config.unwrap();
    assert_eq!((config.min, config.max, config.decimals), (0.0, 10.0, 2));
    assert_eq!(config.pass, GradeConfig::default().pass);
}

// ------------------------------------------------------------------ carga inválida

#[test]
fn loading_rejects_broken_data() {
    let a = Node::new("A", None);
    let mut dup = Node::new("B", None);
    dup.id = a.id;
    assert_eq!(
        Tree::from_nodes([a.clone(), dup]).unwrap_err(),
        TreeError::DuplicateId(a.id)
    );

    let ghost = NodeId::new();
    let orphan = Node::new("Huérfano", Some(ghost));
    assert_eq!(
        Tree::from_nodes([orphan.clone()]).unwrap_err(),
        TreeError::MissingParent {
            node: orphan.id,
            parent: ghost
        }
    );

    // A ↔ B
    let mut x = Node::new("X", None);
    let y = Node::new("Y", Some(x.id));
    x.parent = Some(y.id);
    assert!(matches!(
        Tree::from_nodes([x, y]).unwrap_err(),
        TreeError::Cycle(_)
    ));

    let mut bad = Node::new("Mala escala", None);
    bad.config = Some(GradeConfig {
        min: 5.0,
        max: 5.0,
        ..GradeConfig::default()
    });
    assert_eq!(
        Tree::from_nodes([bad]).unwrap_err(),
        TreeError::Config(ConfigError::EmptyScale)
    );
}
