//! Pruebas de la Fase 3: nota objetivo y nota mínima necesaria.

use notable_core::{
    Aggregation, GradeConfig, MissingGrade, Node, NodeId, Requirement, Rounding, Tree, Warning,
};

/// Crea hojas con nota bajo `parent`.
fn leaves(tree: &mut Tree, parent: NodeId, grades: &[Option<f64>]) -> Vec<NodeId> {
    grades
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let id = tree
                .create(Some(parent), format!("Nota {}", i + 1))
                .unwrap();
            tree.set_grade(id, *g).unwrap();
            id
        })
        .collect()
}

fn weighted_leaf(tree: &mut Tree, parent: NodeId, weight: f64, grade: Option<f64>) -> NodeId {
    let id = tree.create(Some(parent), "Evaluación").unwrap();
    tree.set_weight(id, Some(weight)).unwrap();
    tree.set_grade(id, grade).unwrap();
    id
}

fn needed(tree: &Tree, id: NodeId) -> f64 {
    match tree.requirement(id).unwrap() {
        Some(Requirement::Needed { grade, .. }) => grade,
        other => panic!("esperaba Needed, salió {other:?}"),
    }
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "esperaba {expected}, salió {actual}"
    );
}

/// Ramo 1–7 con notas 4,0 y 4,0 y una tercera sin nota; objetivo 4,0.
fn course_4_4_missing(rounding: Rounding) -> (Tree, NodeId, NodeId) {
    let mut tree = Tree::new();
    let course = tree.create(None, "Cálculo").unwrap();
    tree.set_config(
        course,
        Some(GradeConfig {
            rounding,
            ..GradeConfig::SCALE_1_7
        }),
    )
    .unwrap();
    let ids = leaves(&mut tree, course, &[Some(4.0), Some(4.0), None]);
    tree.set_target(course, Some(4.0)).unwrap();
    (tree, course, ids[2])
}

// ------------------------------------------------------------------ objetivo

#[test]
fn no_target_no_requirement() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Historia").unwrap();
    leaves(&mut tree, course, &[Some(4.0), None]);
    assert_eq!(tree.requirement(course).unwrap(), None);
}

#[test]
fn target_validation_and_serialization() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Historia").unwrap();
    assert!(tree.set_target(course, Some(f64::NAN)).is_err());
    tree.set_target(course, Some(4.0)).unwrap();
    assert!(tree.is_dirty(course));

    let node = tree.get(course).unwrap();
    let json = node.to_json().unwrap();
    assert!(json.contains(r#""target":4.0"#), "{json}");
    assert_eq!(&Node::from_json(&json).unwrap(), node);

    tree.set_target(course, None).unwrap();
    assert!(
        !tree
            .get(course)
            .unwrap()
            .to_json()
            .unwrap()
            .contains("target")
    );
}

#[test]
fn target_out_of_scale_warns_and_is_unreachable() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Historia").unwrap();
    leaves(&mut tree, course, &[Some(6.0), None]);
    tree.set_target(course, Some(8.0)).unwrap();

    let eval = tree.evaluate();
    assert!(
        eval.warnings_for(course)
            .any(|w| matches!(w, Warning::TargetOutOfScale { target, .. } if *target == 8.0))
    );
    assert_eq!(
        tree.requirement(course).unwrap(),
        Some(Requirement::Unreachable)
    );
}

#[test]
fn target_node_rounds_its_final_grade() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Historia").unwrap();
    leaves(&mut tree, course, &[Some(4.0), Some(3.9)]);
    // Sin objetivo no se redondea…
    close(tree.evaluate().grade(course).unwrap(), 3.95);
    // …con objetivo, sí: 3,95 → 4,0, y el objetivo está cumplido.
    tree.set_target(course, Some(4.0)).unwrap();
    close(tree.evaluate().grade(course).unwrap(), 4.0);
    assert_eq!(
        tree.requirement(course).unwrap(),
        Some(Requirement::AlreadyMet)
    );
    // El usuario puede pedir que no se redondee.
    tree.set_round_result(course, Some(false)).unwrap();
    close(tree.evaluate().grade(course).unwrap(), 3.95);
    assert_eq!(
        tree.requirement(course).unwrap(),
        Some(Requirement::Unreachable)
    );
}

// ------------------------------------------------------------------ una nota que falta

#[test]
fn exact_at_the_limit_without_rounding() {
    let (mut tree, course, missing) = course_4_4_missing(Rounding::Round);
    tree.set_round_result(course, Some(false)).unwrap();
    // (4 + 4 + x) / 3 ≥ 4 → x = 4,0 justo.
    assert_eq!(
        tree.requirement(course).unwrap(),
        Some(Requirement::Needed {
            grade: 4.0,
            missing: vec![MissingGrade {
                node: missing,
                grade: 4.0
            }],
        })
    );
    // Con 3,9 no alcanza.
    tree.set_grade(missing, Some(3.9)).unwrap();
    assert!(tree.evaluate().grade(course).unwrap() < 4.0);
}

#[test]
fn rounding_vs_truncating() {
    // Aproximando, 3,95 ya es 4,0: basta (8 + x) / 3 ≥ 3,95 → x ≥ 3,85 → 3,9.
    let (tree, course, _) = course_4_4_missing(Rounding::Round);
    close(needed(&tree, course), 3.9);

    // Truncando hay que llegar a 4,0 de verdad.
    let (tree, course, _) = course_4_4_missing(Rounding::Truncate);
    close(needed(&tree, course), 4.0);
}

#[test]
fn needed_grade_is_one_you_can_get() {
    // (5,0 + x) / 2 ≥ 4,52 → x ≥ 4,04: con un decimal hace falta 4,1, no 4,04.
    let mut tree = Tree::new();
    let course = tree.create(None, "Física").unwrap();
    leaves(&mut tree, course, &[Some(5.0), None]);
    tree.set_round_result(course, Some(false)).unwrap();
    tree.set_target(course, Some(4.52)).unwrap();
    close(needed(&tree, course), 4.1);

    // Con dos decimales en la escala, 4,04.
    tree.set_config(
        course,
        Some(GradeConfig {
            decimals: 2,
            ..GradeConfig::SCALE_1_7
        }),
    )
    .unwrap();
    close(needed(&tree, course), 4.04);
}

#[test]
fn usm_weighted_course_0_100() {
    // 30 % · 50 + 30 % · 60 + 40 % · x ≥ 54,5 (se aproxima a 55) → x ≥ 53,75 → 54.
    let mut tree = Tree::new();
    let course = tree.create(None, "Matemática USM").unwrap();
    tree.set_config(course, Some(GradeConfig::SCALE_0_100))
        .unwrap();
    tree.set_aggregation(course, Aggregation::Weighted).unwrap();
    weighted_leaf(&mut tree, course, 30.0, Some(50.0));
    weighted_leaf(&mut tree, course, 30.0, Some(60.0));
    let exam = weighted_leaf(&mut tree, course, 40.0, None);
    tree.set_target(course, Some(55.0)).unwrap();

    close(needed(&tree, course), 54.0);
    tree.set_grade(exam, Some(54.0)).unwrap();
    close(tree.evaluate().grade(course).unwrap(), 55.0);
    tree.set_grade(exam, Some(53.0)).unwrap();
    close(tree.evaluate().grade(course).unwrap(), 54.0);
}

#[test]
fn already_met_with_minimum_grade() {
    // (7 + 7 + 1) / 3 = 5 ≥ 4: aunque saque un 1,0, aprueba.
    let mut tree = Tree::new();
    let course = tree.create(None, "Arte").unwrap();
    let ids = leaves(&mut tree, course, &[Some(7.0), Some(7.0), None]);
    tree.set_target(course, Some(4.0)).unwrap();
    assert_eq!(
        tree.requirement(course).unwrap(),
        Some(Requirement::AlreadyMet)
    );

    // Completo y cumplido.
    tree.set_grade(ids[2], Some(2.0)).unwrap();
    assert_eq!(
        tree.requirement(course).unwrap(),
        Some(Requirement::AlreadyMet)
    );
}

#[test]
fn unreachable_even_with_maximum() {
    // (1 + 1 + 7) / 3 = 3 < 4.
    let mut tree = Tree::new();
    let course = tree.create(None, "Química").unwrap();
    let ids = leaves(&mut tree, course, &[Some(1.0), Some(1.0), None]);
    tree.set_target(course, Some(4.0)).unwrap();
    assert_eq!(
        tree.requirement(course).unwrap(),
        Some(Requirement::Unreachable)
    );

    // Completo y bajo el objetivo.
    tree.set_grade(ids[2], Some(7.0)).unwrap();
    assert_eq!(
        tree.requirement(course).unwrap(),
        Some(Requirement::Unreachable)
    );
}

#[test]
fn reaching_needs_the_maximum() {
    // (1 + 7 + x) / 3 ≥ 5 → x = 7,0.
    let mut tree = Tree::new();
    let course = tree.create(None, "Química").unwrap();
    leaves(&mut tree, course, &[Some(1.0), Some(7.0), None]);
    tree.set_round_result(course, Some(false)).unwrap();
    tree.set_target(course, Some(5.0)).unwrap();
    close(needed(&tree, course), 7.0);
}

#[test]
fn missing_grade_with_zero_weight_cannot_help() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Química").unwrap();
    tree.set_aggregation(course, Aggregation::Weighted).unwrap();
    weighted_leaf(&mut tree, course, 100.0, Some(3.0));
    weighted_leaf(&mut tree, course, 0.0, None);
    tree.set_target(course, Some(4.0)).unwrap();
    assert_eq!(
        tree.requirement(course).unwrap(),
        Some(Requirement::Unreachable)
    );
}

#[test]
fn target_on_a_leaf() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Ramo").unwrap();
    let exam = tree.create(Some(course), "Examen").unwrap();
    tree.set_target(exam, Some(5.0)).unwrap();
    close(needed(&tree, exam), 5.0);
    tree.set_grade(exam, Some(5.5)).unwrap();
    assert_eq!(
        tree.requirement(exam).unwrap(),
        Some(Requirement::AlreadyMet)
    );
}

// ------------------------------------------------------------------ redondeo intermedio

/// Ramo ponderado 50/50: sección A (4,0 y una sin nota) y sección B (hoja con 3,0).
fn sections(round_every_level: bool) -> (Tree, NodeId) {
    let mut tree = Tree::new();
    let course = tree.create(None, "Programación").unwrap();
    tree.set_config(
        course,
        Some(GradeConfig {
            round_every_level,
            ..GradeConfig::SCALE_1_7
        }),
    )
    .unwrap();
    tree.set_aggregation(course, Aggregation::Weighted).unwrap();
    let a = tree.create(Some(course), "Sección A").unwrap();
    tree.set_weight(a, Some(50.0)).unwrap();
    leaves(&mut tree, a, &[Some(4.0), None]);
    weighted_leaf(&mut tree, course, 50.0, Some(3.0));
    tree.set_target(course, Some(4.0)).unwrap();
    (tree, course)
}

#[test]
fn intermediate_rounding_changes_the_minimum() {
    // Redondeando cada nivel: A = (4 + x)/2 → 4,9 basta (A + 3)/2 = 3,95 → 4,0.
    // (4 + 5,7)/2 = 4,85 → 4,9. Con 5,6: 4,8 → 3,9.
    let (tree, course) = sections(true);
    close(needed(&tree, course), 5.7);

    // Solo la nota final: (4 + x)/4 + 1,5 ≥ 3,95 → x ≥ 5,8.
    let (tree, course) = sections(false);
    close(needed(&tree, course), 5.8);
}

// ------------------------------------------------------------------ varias notas faltan

#[test]
fn several_missing_assume_the_same_grade() {
    // (3 + 4 + x + x) / 4 ≥ 4 → x ≥ 4,5.
    let mut tree = Tree::new();
    let course = tree.create(None, "Biología").unwrap();
    let ids = leaves(&mut tree, course, &[Some(3.0), Some(4.0), None, None]);
    tree.set_round_result(course, Some(false)).unwrap();
    tree.set_target(course, Some(4.0)).unwrap();
    assert_eq!(
        tree.requirement(course).unwrap(),
        Some(Requirement::Needed {
            grade: 4.5,
            missing: vec![
                MissingGrade {
                    node: ids[2],
                    grade: 4.5
                },
                MissingGrade {
                    node: ids[3],
                    grade: 4.5
                },
            ],
        })
    );
}

#[test]
fn missing_grades_in_nested_nodes() {
    // Ramo: Tareas (sin notas aún, dos hojas) y Certamen 3,0; promedio simple de ambos.
    // (3 + x) / 2 ≥ 4 → x ≥ 5,0 en cada tarea.
    let mut tree = Tree::new();
    let course = tree.create(None, "Economía").unwrap();
    let homework = tree.create(Some(course), "Tareas").unwrap();
    let hw = leaves(&mut tree, homework, &[None, None]);
    leaves(&mut tree, course, &[Some(3.0)]);
    tree.set_round_result(course, Some(false)).unwrap();
    tree.set_target(course, Some(4.0)).unwrap();

    match tree.requirement(course).unwrap() {
        Some(Requirement::Needed { grade, missing }) => {
            close(grade, 5.0);
            let nodes: Vec<NodeId> = missing.iter().map(|m| m.node).collect();
            assert_eq!(nodes, hw);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn mixed_scales_use_each_scale() {
    // Ramo 1–7: una nota 4,0 y un informe 0–100 sin nota. Sin redondeo:
    // (4 + (1 + 6·x/100)) / 2 ≥ 4,5 → x ≥ 66,67 → 67.
    let mut tree = Tree::new();
    let course = tree.create(None, "Laboratorio").unwrap();
    leaves(&mut tree, course, &[Some(4.0)]);
    let report = tree.create(Some(course), "Informe").unwrap();
    tree.set_config(report, Some(GradeConfig::SCALE_0_100))
        .unwrap();
    tree.set_round_result(course, Some(false)).unwrap();
    tree.set_target(course, Some(4.5)).unwrap();
    close(needed(&tree, course), 67.0);

    // Dos que faltan en escalas distintas: mismo lugar relativo de cada escala.
    // (1 + 6t) + (1 + 6t) = 2 · 4,5 → t = 7/12: 4,5 en 1–7 y 58,33 → 59 en 0–100.
    let mut tree = Tree::new();
    let course = tree.create(None, "Laboratorio").unwrap();
    let a = tree.create(Some(course), "Prueba").unwrap();
    let b = tree.create(Some(course), "Informe").unwrap();
    tree.set_config(b, Some(GradeConfig::SCALE_0_100)).unwrap();
    tree.set_round_result(course, Some(false)).unwrap();
    tree.set_target(course, Some(4.5)).unwrap();
    match tree.requirement(course).unwrap() {
        Some(Requirement::Needed { grade, missing }) => {
            close(grade, 4.5);
            assert_eq!(missing[0].node, a);
            assert_eq!(missing[1].node, b);
            close(missing[1].grade, 59.0);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn requirement_does_not_change_the_tree() {
    let (mut tree, course, missing) = course_4_4_missing(Rounding::Round);
    tree.take_dirty();
    tree.requirement(course).unwrap();
    assert!(tree.take_dirty().is_empty());
    assert_eq!(tree.get(missing).unwrap().grade, None);
    assert!(tree.evaluate().warnings().is_empty());
}
