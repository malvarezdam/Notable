//! Pruebas de la Fase 2: motor de cálculo.

use notable_core::{Aggregation, Conversion, GradeConfig, NodeId, Rounding, Tree, Warning};

fn close(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("debería haber nota");
    assert!(
        (actual - expected).abs() < 1e-9,
        "esperaba {expected}, salió {actual}"
    );
}

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

// ------------------------------------------------------------------ promedio simple

#[test]
fn simple_average_1_to_7() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Historia").unwrap();
    leaves(&mut tree, course, &[Some(4.5), Some(5.0), Some(3.8)]);

    let eval = tree.evaluate();
    // Por defecto no se redondea nada.
    close(eval.grade(course), 13.3 / 3.0);
    assert!(eval.get(course).unwrap().complete);
    assert!(eval.warnings().is_empty());

    tree.set_round_result(course, Some(true)).unwrap();
    let eval = tree.evaluate();
    close(eval.grade(course), 4.4);
    close(eval.get(course).unwrap().unrounded, 13.3 / 3.0);
}

#[test]
fn partial_average_ignores_missing_grades() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Física").unwrap();
    let ids = leaves(&mut tree, course, &[Some(4.0), Some(6.0), None]);

    let eval = tree.evaluate();
    close(eval.grade(course), 5.0);
    assert!(!eval.get(course).unwrap().complete);
    assert_eq!(eval.grade(ids[2]), None);

    let empty = tree.create(None, "Sin notas").unwrap();
    tree.create(Some(empty), "Certamen").unwrap();
    assert_eq!(tree.evaluate().grade(empty), None);
}

#[test]
fn simple_average_ignores_weights() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Química").unwrap();
    weighted_leaf(&mut tree, course, 90.0, Some(7.0));
    weighted_leaf(&mut tree, course, 10.0, Some(3.0));
    close(tree.evaluate().grade(course), 5.0);

    tree.set_aggregation(course, Aggregation::Weighted).unwrap();
    close(tree.evaluate().grade(course), 6.6);
}

#[test]
fn deleted_nodes_do_not_count() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Química").unwrap();
    let ids = leaves(&mut tree, course, &[Some(7.0), Some(1.0)]);
    tree.delete(ids[1]).unwrap();
    let eval = tree.evaluate();
    close(eval.grade(course), 7.0);
    assert!(eval.get(ids[1]).is_none());
}

// ------------------------------------------------------------------ ramo USM 0–100

/// Cálculo (0–100, ponderado): Certamen 1 30 %, Certamen 2 30 %, Tareas 40 % (promedio
/// simple de T1 y T2).
fn usm_course(c2: Option<f64>) -> (Tree, NodeId, NodeId) {
    let mut tree = Tree::new();
    let course = tree.create(None, "MAT021").unwrap();
    tree.set_config(course, Some(GradeConfig::SCALE_0_100))
        .unwrap();
    tree.set_aggregation(course, Aggregation::Weighted).unwrap();
    weighted_leaf(&mut tree, course, 30.0, Some(55.0));
    weighted_leaf(&mut tree, course, 30.0, c2);
    let homework = tree.create(Some(course), "Tareas").unwrap();
    tree.set_weight(homework, Some(40.0)).unwrap();
    leaves(&mut tree, homework, &[Some(80.0), Some(91.0)]);
    (tree, course, homework)
}

#[test]
fn usm_weighted_course() {
    let (mut tree, course, homework) = usm_course(Some(70.0));
    let eval = tree.evaluate();
    close(eval.grade(homework), 85.5);
    // 0,3·55 + 0,3·70 + 0,4·85,5
    close(eval.grade(course), 71.7);
    assert!(eval.warnings().is_empty());

    tree.set_round_result(course, Some(true)).unwrap();
    close(tree.evaluate().grade(course), 72.0);
}

#[test]
fn usm_partial_grade_uses_only_graded_weights() {
    let (tree, course, _) = usm_course(None);
    let eval = tree.evaluate();
    // (0,3·55 + 0,4·85,5) / 0,7
    close(eval.grade(course), (16.5 + 34.2) / 0.7);
    assert!(!eval.get(course).unwrap().complete);
}

#[test]
fn share_within_ancestor() {
    let (tree, course, homework) = usm_course(Some(70.0));
    let eval = tree.evaluate();
    let t1 = tree.children(Some(homework)).next().unwrap().id;
    close(eval.share_within(&tree, t1, course), 0.2);
    close(eval.share_within(&tree, homework, course), 0.4);
    close(eval.share_within(&tree, course, course), 1.0);
    assert_eq!(eval.share_within(&tree, course, t1), None);
}

// ------------------------------------------------------------------ redondeo

#[test]
fn intermediate_rounding_is_optional() {
    // Controles (4,0 y 4,5 → 4,25) y Certamen 4,6, promedio simple, 1 decimal.
    let mut tree = Tree::new();
    let course = tree.create(None, "Álgebra").unwrap();
    let quizzes = tree.create(Some(course), "Controles").unwrap();
    leaves(&mut tree, quizzes, &[Some(4.0), Some(4.5)]);
    leaves(&mut tree, course, &[Some(4.6)]);
    tree.set_round_result(course, Some(true)).unwrap();

    // Solo se redondea el ramo: (4,25 + 4,6) / 2 = 4,425 → 4,4.
    let eval = tree.evaluate();
    close(eval.grade(quizzes), 4.25);
    close(eval.grade(course), 4.4);

    // Redondeando cada nivel: controles 4,3 → (4,3 + 4,6) / 2 = 4,45 → 4,5.
    let config = GradeConfig {
        round_every_level: true,
        ..GradeConfig::SCALE_1_7
    };
    tree.set_config(course, Some(config)).unwrap();
    let eval = tree.evaluate();
    close(eval.grade(quizzes), 4.3);
    close(eval.get(quizzes).unwrap().unrounded, 4.25);
    close(eval.grade(course), 4.5);

    // Un nivel puede excluirse aunque la configuración diga "cada nivel".
    tree.set_round_result(quizzes, Some(false)).unwrap();
    close(tree.evaluate().grade(course), 4.4);
}

#[test]
fn truncate_versus_round() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Programación").unwrap();
    leaves(&mut tree, course, &[Some(4.4), Some(4.5)]); // 4,45
    tree.set_round_result(course, Some(true)).unwrap();
    close(tree.evaluate().grade(course), 4.5);

    let truncate = GradeConfig::new(1.0, 7.0, 4.0, 1, Rounding::Truncate).unwrap();
    tree.set_config(course, Some(truncate)).unwrap();
    close(tree.evaluate().grade(course), 4.4);
}

#[test]
fn rounding_survives_floating_point_noise() {
    let round = GradeConfig::SCALE_1_7;
    let truncate = GradeConfig {
        rounding: Rounding::Truncate,
        ..round
    };
    // 4,45 y 1,005 no se pueden representar exactos en binario.
    assert_eq!(round.apply_rounding(4.45), 4.5);
    assert_eq!(round.apply_rounding(4.449), 4.4);
    assert_eq!(truncate.apply_rounding(4.3), 4.3);
    assert_eq!(truncate.apply_rounding(0.1 + 0.2), 0.3);
    assert_eq!(truncate.apply_rounding(4.99), 4.9);
    let two = GradeConfig {
        decimals: 2,
        ..round
    };
    assert_eq!(two.apply_rounding(1.005), 1.01);
    assert_eq!(GradeConfig::SCALE_0_100.apply_rounding(54.5), 55.0);
}

#[test]
fn entered_grades_are_never_rounded() {
    let config = GradeConfig {
        round_every_level: true,
        ..GradeConfig::SCALE_1_7
    };
    let mut tree = Tree::new();
    let course = tree.create(None, "Ramo").unwrap();
    tree.set_config(course, Some(config)).unwrap();
    let ids = leaves(&mut tree, course, &[Some(4.44), Some(4.44)]);
    let eval = tree.evaluate();
    close(eval.grade(ids[0]), 4.44);
    close(eval.grade(course), 4.4);
}

// ------------------------------------------------------------------ escalas mezcladas

/// Semestre 1–7 (simple) con un ramo 0–100 (71,7) y un ramo 1–7 (5,0).
fn mixed_semester(conversion: Conversion) -> (Tree, NodeId, NodeId) {
    let mut tree = Tree::new();
    let semester = tree.create(None, "Semestre").unwrap();
    let course = tree.create(Some(semester), "MAT021").unwrap();
    let config = GradeConfig {
        conversion,
        ..GradeConfig::SCALE_0_100
    };
    tree.set_config(course, Some(config)).unwrap();
    leaves(&mut tree, course, &[Some(71.7)]);
    let other = tree.create(Some(semester), "Historia").unwrap();
    leaves(&mut tree, other, &[Some(5.0)]);
    (tree, semester, course)
}

#[test]
fn mixed_scales_linear() {
    let (tree, semester, course) = mixed_semester(Conversion::Linear);
    let eval = tree.evaluate();
    close(eval.grade(course), 71.7); // en su propia escala
    // 71,7 de 100 → 1 + 0,717·6 = 5,302
    close(eval.grade(semester), (5.302 + 5.0) / 2.0);
}

#[test]
fn mixed_scales_by_pass() {
    let (tree, semester, _) = mixed_semester(Conversion::ByPass);
    // 71,7 de 100 con aprobación 55 → 4 + (16,7 / 45)·3
    let converted = 4.0 + 16.7 / 45.0 * 3.0;
    close(tree.evaluate().grade(semester), (converted + 5.0) / 2.0);
}

#[test]
fn conversion_uses_the_rounded_child() {
    let (mut tree, semester, course) = mixed_semester(Conversion::Linear);
    tree.set_round_result(course, Some(true)).unwrap();
    // 72 de 100 → 5,32
    close(tree.evaluate().grade(semester), (5.32 + 5.0) / 2.0);
}

#[test]
fn conversion_formulas() {
    let hundred = GradeConfig {
        conversion: Conversion::ByPass,
        ..GradeConfig::SCALE_0_100
    };
    let seven = GradeConfig::SCALE_1_7;
    assert_eq!(hundred.convert_to(55.0, &seven), 4.0);
    assert_eq!(hundred.convert_to(0.0, &seven), 1.0);
    assert_eq!(hundred.convert_to(100.0, &seven), 7.0);
    assert!((hundred.convert_to(77.5, &seven) - 5.5).abs() < 1e-12);
    // Al revés, lineal: 4,0 de 7 → 50 de 100.
    assert_eq!(seven.convert_to(4.0, &GradeConfig::SCALE_0_100), 50.0);
    // Misma escala: no cambia.
    assert_eq!(seven.convert_to(4.37, &seven), 4.37);
}

// ------------------------------------------------------------------ pesos "respecto a"

/// Ramo ponderado: Certámenes 60 % (simple), Tareas 40 % (ponderado). Cada tarea dice cuánto
/// vale del ramo.
fn course_with_relative_weights(t1: f64, t2: f64) -> (Tree, NodeId, NodeId, NodeId, NodeId) {
    let mut tree = Tree::new();
    let course = tree.create(None, "Cálculo").unwrap();
    tree.set_aggregation(course, Aggregation::Weighted).unwrap();
    let exams = tree.create(Some(course), "Certámenes").unwrap();
    tree.set_weight(exams, Some(60.0)).unwrap();
    leaves(&mut tree, exams, &[Some(4.0), Some(5.0)]);
    let homework = tree.create(Some(course), "Tareas").unwrap();
    tree.set_weight(homework, Some(40.0)).unwrap();
    tree.set_aggregation(homework, Aggregation::Weighted)
        .unwrap();
    let a = weighted_leaf(&mut tree, homework, t1, Some(7.0));
    let b = weighted_leaf(&mut tree, homework, t2, Some(5.0));
    tree.set_weight_base(a, Some(course)).unwrap();
    tree.set_weight_base(b, Some(course)).unwrap();
    (tree, course, homework, a, b)
}

#[test]
fn weight_relative_to_ancestor() {
    // T1 vale 10 % del ramo y T2 30 % → dentro de Tareas (40 %) son 25 % y 75 %.
    let (tree, course, homework, a, b) = course_with_relative_weights(10.0, 30.0);
    let eval = tree.evaluate();
    assert!(eval.warnings().is_empty(), "{:?}", eval.warnings());
    close(Some(eval.get(a).unwrap().weight), 25.0);
    close(Some(eval.get(b).unwrap().weight), 75.0);
    close(eval.grade(homework), 0.25 * 7.0 + 0.75 * 5.0);
    close(eval.share_within(&tree, a, course), 0.1);
    close(eval.grade(course), 0.6 * 4.5 + 0.4 * 5.5);
}

#[test]
fn contradictory_relative_weights() {
    // 10 % + 35 % del ramo, pero Tareas vale 40 %: no cuadra.
    let (tree, _, homework, _, _) = course_with_relative_weights(10.0, 35.0);
    let eval = tree.evaluate();
    let warnings: Vec<_> = eval.warnings_for(homework).collect();
    assert_eq!(warnings.len(), 1);
    match warnings[0] {
        Warning::ContradictoryWeights { total, .. } => assert!((total - 112.5).abs() < 1e-9),
        w => panic!("aviso inesperado: {w:?}"),
    }
    // Igual se calcula, normalizando.
    assert!(eval.grade(homework).is_some());
}

#[test]
fn weight_base_must_be_an_ancestor() {
    let (mut tree, _, homework, a, b) = course_with_relative_weights(10.0, 30.0);
    assert!(tree.set_weight_base(a, Some(b)).is_err());
    // Respecto al padre = sin base.
    tree.set_weight_base(a, Some(homework)).unwrap();
    assert_eq!(tree.get(a).unwrap().weight_base, None);
}

#[test]
fn moved_node_with_stale_base_warns() {
    let (mut tree, _, _, a, _) = course_with_relative_weights(10.0, 30.0);
    let other = tree.create(None, "Otro ramo").unwrap();
    tree.set_aggregation(other, Aggregation::Weighted).unwrap();
    tree.move_node(a, Some(other), 0).unwrap();
    let eval = tree.evaluate();
    assert!(
        eval.warnings_for(a)
            .any(|w| matches!(w, Warning::InvalidWeightBase { .. }))
    );
    // Se usó el peso tal cual, respecto al nuevo padre.
    close(Some(eval.get(a).unwrap().weight), 10.0);
}

#[test]
fn relative_weight_under_zero_weight_parent() {
    let (mut tree, _, homework, a, _) = course_with_relative_weights(10.0, 30.0);
    tree.set_weight(homework, Some(0.0)).unwrap();
    let eval = tree.evaluate();
    assert!(
        eval.warnings_for(a)
            .any(|w| matches!(w, Warning::UnconvertibleWeight { .. }))
    );
}

#[test]
fn relative_weight_through_simple_level() {
    // Ramo simple con 2 hijos: Unidad 1 vale 50 % del ramo. Dentro, un control dice valer
    // 10 % del ramo → 20 % de la unidad.
    let mut tree = Tree::new();
    let course = tree.create(None, "Ramo").unwrap();
    let unit = tree.create(Some(course), "Unidad 1").unwrap();
    tree.create(Some(course), "Unidad 2").unwrap();
    tree.set_aggregation(unit, Aggregation::Weighted).unwrap();
    let quiz = weighted_leaf(&mut tree, unit, 10.0, Some(6.0));
    tree.set_weight_base(quiz, Some(course)).unwrap();
    weighted_leaf(&mut tree, unit, 80.0, Some(4.0));
    let eval = tree.evaluate();
    close(Some(eval.get(quiz).unwrap().weight), 20.0);
    assert!(eval.warnings().is_empty(), "{:?}", eval.warnings());
}

// ------------------------------------------------------------------ avisos

#[test]
fn weights_that_dont_sum_100_warn_but_still_compute() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Ramo").unwrap();
    tree.set_aggregation(course, Aggregation::Weighted).unwrap();
    weighted_leaf(&mut tree, course, 30.0, Some(4.0));
    weighted_leaf(&mut tree, course, 30.0, Some(6.0));
    weighted_leaf(&mut tree, course, 30.0, Some(5.0));
    let eval = tree.evaluate();
    assert_eq!(
        eval.warnings(),
        [Warning::WeightsDontSum {
            node: course,
            total: 90.0
        }]
    );
    close(eval.grade(course), 5.0);
}

#[test]
fn missing_weight_counts_as_zero() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Ramo").unwrap();
    tree.set_aggregation(course, Aggregation::Weighted).unwrap();
    weighted_leaf(&mut tree, course, 100.0, Some(4.0));
    let unweighted = tree.create(Some(course), "Sin peso").unwrap();
    tree.set_grade(unweighted, Some(7.0)).unwrap();
    let eval = tree.evaluate();
    assert_eq!(
        eval.warnings(),
        [Warning::MissingWeight { node: unweighted }]
    );
    close(eval.grade(course), 4.0);
}

#[test]
fn grade_out_of_scale_warns() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Ramo").unwrap();
    let ids = leaves(&mut tree, course, &[Some(8.0), Some(6.0)]);
    let eval = tree.evaluate();
    assert_eq!(
        eval.warnings(),
        [Warning::GradeOutOfScale {
            node: ids[0],
            grade: 8.0
        }]
    );
    close(eval.grade(course), 7.0);
}
