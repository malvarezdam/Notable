//! Pruebas de la Fase 4: reglas opcionales (cada una apagada y encendida).

use notable_core::{
    Aggregation, Effect, Exemption, Minimum, Node, NodeId, ReplaceTarget, Replacement, Requirement,
    Rules, Tree, TreeError, Warning,
};

fn close(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("debería haber nota");
    assert!(
        (actual - expected).abs() < 1e-9,
        "esperaba {expected}, salió {actual}"
    );
}

fn leaf(tree: &mut Tree, parent: NodeId, name: &str, grade: Option<f64>) -> NodeId {
    let id = tree.create(Some(parent), name).unwrap();
    tree.set_grade(id, grade).unwrap();
    id
}

fn effect(tree: &Tree, id: NodeId) -> Option<Effect> {
    tree.evaluate().get(id).unwrap().effect
}

fn needed(tree: &Tree, id: NodeId) -> f64 {
    match tree.requirement(id).unwrap() {
        Some(Requirement::Needed { grade, .. }) => grade,
        other => panic!("esperaba Needed, salió {other:?}"),
    }
}

fn drop_lowest(n: u32) -> Rules {
    Rules {
        drop_lowest: n,
        ..Rules::default()
    }
}

fn replaces(target: ReplaceTarget) -> Rules {
    Rules {
        replaces: Some(Replacement {
            target,
            ..Replacement::default()
        }),
        ..Rules::default()
    }
}

fn exemption(threshold: f64, keep_if_better: bool) -> Rules {
    Rules {
        exemption: Some(Exemption {
            threshold,
            keep_if_better,
        }),
        ..Rules::default()
    }
}

fn minimum(grade: f64, cap: Option<f64>) -> Rules {
    Rules {
        minimum: Some(Minimum { grade, cap }),
        ..Rules::default()
    }
}

// ------------------------------------------------------------------ por defecto

#[test]
fn rules_are_off_by_default_and_not_serialized() {
    let node = Node::new("Ramo", None);
    assert!(node.rules.is_off());
    let json = node.to_json().unwrap();
    assert!(
        !json.contains("rules") && !json.contains("credits"),
        "{json}"
    );

    let mut node = node;
    node.rules = drop_lowest(1);
    node.credits = Some(6.0);
    let json = node.to_json().unwrap();
    assert!(json.contains(r#""rules":{"drop_lowest":1}"#), "{json}");
    assert_eq!(Node::from_json(&json).unwrap(), node);
}

// ------------------------------------------------------------------ eliminar las peores

#[test]
fn drop_lowest_grades() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Cálculo").unwrap();
    let worst = leaf(&mut tree, course, "C1", Some(3.0));
    leaf(&mut tree, course, "C2", Some(5.0));
    leaf(&mut tree, course, "C3", Some(6.0));

    close(tree.evaluate().grade(course), 14.0 / 3.0);
    assert_eq!(effect(&tree, worst), None);

    tree.set_rules(course, drop_lowest(1)).unwrap();
    close(tree.evaluate().grade(course), 5.5);
    assert_eq!(effect(&tree, worst), Some(Effect::Dropped));

    // Nunca se eliminan todas.
    tree.set_rules(course, drop_lowest(10)).unwrap();
    close(tree.evaluate().grade(course), 6.0);
}

#[test]
fn drop_lowest_in_weighted_and_with_missing() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Física").unwrap();
    tree.set_aggregation(course, Aggregation::Weighted).unwrap();
    for (w, g) in [(30.0, Some(2.0)), (70.0, Some(6.0)), (50.0, None)] {
        let id = leaf(&mut tree, course, "Ev", g);
        tree.set_weight(id, Some(w)).unwrap();
    }
    tree.set_rules(course, drop_lowest(1)).unwrap();
    // Se elimina el 2,0 (la peor nota, no el menor peso); la sin nota no cuenta todavía.
    close(tree.evaluate().grade(course), 6.0);
}

// ------------------------------------------------------------------ reemplazos

#[test]
fn exam_replaces_lowest() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Química").unwrap();
    let c1 = leaf(&mut tree, course, "C1", Some(3.0));
    leaf(&mut tree, course, "C2", Some(5.0));
    let exam = leaf(&mut tree, course, "Examen", Some(6.0));

    // Apagado: el examen es una nota más.
    close(tree.evaluate().grade(course), 14.0 / 3.0);

    tree.set_rules(exam, replaces(ReplaceTarget::Lowest))
        .unwrap();
    close(tree.evaluate().grade(course), 5.5);
    let eval = tree.evaluate();
    assert_eq!(
        eval.get(c1).unwrap().effect,
        Some(Effect::Replaced { by: exam })
    );
    assert_eq!(
        eval.get(exam).unwrap().effect,
        Some(Effect::Replaces { node: c1 })
    );

    // Si no mejora, no reemplaza (por defecto).
    tree.set_grade(exam, Some(2.0)).unwrap();
    close(tree.evaluate().grade(course), 4.0);
    assert_eq!(effect(&tree, exam), Some(Effect::Unused));

    // Reemplazar siempre.
    let mut rules = replaces(ReplaceTarget::Lowest);
    rules.replaces.as_mut().unwrap().only_if_better = false;
    tree.set_rules(exam, rules).unwrap();
    close(tree.evaluate().grade(course), 3.5);

    // Reemplaza y además cuenta como nota propia.
    tree.set_grade(exam, Some(6.0)).unwrap();
    let mut rules = replaces(ReplaceTarget::Lowest);
    rules.replaces.as_mut().unwrap().counts_itself = true;
    tree.set_rules(exam, rules).unwrap();
    close(tree.evaluate().grade(course), 17.0 / 3.0);
}

#[test]
fn makeup_test_fills_a_missing_grade() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Historia").unwrap();
    leaf(&mut tree, course, "C1", Some(4.0));
    let c2 = leaf(&mut tree, course, "C2", None);
    let makeup = leaf(&mut tree, course, "Recuperativa", None);
    tree.set_rules(makeup, replaces(ReplaceTarget::Node(c2)))
        .unwrap();

    // La recuperativa sin nota no hace falta para estar completo, pero C2 sí.
    let eval = tree.evaluate();
    close(eval.grade(course), 4.0);
    assert!(!eval.get(course).unwrap().complete);

    tree.set_grade(makeup, Some(5.0)).unwrap();
    let eval = tree.evaluate();
    close(eval.grade(course), 4.5);
    assert!(eval.get(course).unwrap().complete);
}

#[test]
fn invalid_rules_are_rejected() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Ramo").unwrap();
    let a = leaf(&mut tree, course, "A", Some(4.0));
    let other = tree.create(None, "Otro").unwrap();
    let b = leaf(&mut tree, other, "B", Some(4.0));

    let err = Err(TreeError::InvalidRule);
    assert_eq!(tree.set_rules(a, replaces(ReplaceTarget::Node(a))), err);
    assert_eq!(tree.set_rules(a, replaces(ReplaceTarget::Node(b))), err);
    assert_eq!(tree.set_rules(a, minimum(f64::NAN, None)), err);
    let both = Rules {
        exemption: Some(Exemption::default()),
        ..replaces(ReplaceTarget::Lowest)
    };
    assert_eq!(tree.set_rules(a, both), err);
    assert!(tree.set_credits(a, Some(-1.0)).is_err());
}

#[test]
fn replacement_target_moved_away_warns() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Ramo").unwrap();
    let c1 = leaf(&mut tree, course, "C1", Some(3.0));
    leaf(&mut tree, course, "C2", Some(5.0));
    let exam = leaf(&mut tree, course, "Examen", Some(7.0));
    tree.set_rules(exam, replaces(ReplaceTarget::Node(c1)))
        .unwrap();
    close(tree.evaluate().grade(course), 6.0);

    let other = tree.create(None, "Otro").unwrap();
    tree.move_node(c1, Some(other), 0).unwrap();
    let eval = tree.evaluate();
    close(eval.grade(course), 5.0);
    assert!(
        eval.warnings_for(exam)
            .any(|w| matches!(w, Warning::InvalidReplaceTarget { .. }))
    );
}

// ------------------------------------------------------------------ eximición

#[test]
fn exemption_from_exam() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Biología").unwrap();
    let c1 = leaf(&mut tree, course, "C1", Some(6.0));
    leaf(&mut tree, course, "C2", Some(6.0));
    let exam = leaf(&mut tree, course, "Examen", None);

    // Apagado: el examen falta.
    let eval = tree.evaluate();
    close(eval.grade(course), 6.0);
    assert!(!eval.get(course).unwrap().complete);

    tree.set_rules(exam, exemption(5.5, false)).unwrap();
    let eval = tree.evaluate();
    close(eval.grade(course), 6.0);
    assert!(eval.get(course).unwrap().complete);
    assert_eq!(eval.get(exam).unwrap().effect, Some(Effect::Exempt));

    // Eximido, una nota del examen no cuenta…
    tree.set_grade(exam, Some(3.0)).unwrap();
    close(tree.evaluate().grade(course), 6.0);
    // …salvo que se pida contarla si mejora.
    tree.set_rules(exam, exemption(5.5, true)).unwrap();
    close(tree.evaluate().grade(course), 6.0);
    tree.set_grade(exam, Some(7.0)).unwrap();
    close(tree.evaluate().grade(course), 19.0 / 3.0);

    // Bajo el umbral, el examen cuenta.
    tree.set_grade(c1, Some(4.0)).unwrap();
    tree.set_grade(exam, Some(3.0)).unwrap();
    close(tree.evaluate().grade(course), 13.0 / 3.0);
    assert_eq!(effect(&tree, exam), None);
}

// ------------------------------------------------------------------ requisito mínimo

#[test]
fn minimum_requirement_fails_the_parent() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Programación").unwrap();
    leaf(&mut tree, course, "Teoría", Some(6.0));
    let lab = leaf(&mut tree, course, "Laboratorio", Some(3.5));

    let eval = tree.evaluate();
    assert_eq!(eval.get(course).unwrap().passed(), Some(true));

    tree.set_rules(lab, minimum(4.0, None)).unwrap();
    let eval = tree.evaluate();
    let r = eval.get(course).unwrap();
    close(r.grade, 4.75);
    assert!(!r.requirements_met);
    assert_eq!(r.passed(), Some(false));
    assert!(eval.warnings_for(lab).any(|w| matches!(
        w,
        Warning::MinimumNotMet { minimum, .. } if *minimum == 4.0
    )));

    // Con tope, la nota del ramo queda en 3,9.
    tree.set_rules(lab, minimum(4.0, Some(3.9))).unwrap();
    close(tree.evaluate().grade(course), 3.9);

    // Sin nota todavía, el requisito está pendiente (no reprueba).
    tree.set_grade(lab, None).unwrap();
    assert!(tree.evaluate().get(course).unwrap().requirements_met);

    // Cumplido.
    tree.set_grade(lab, Some(4.0)).unwrap();
    assert_eq!(tree.evaluate().get(course).unwrap().passed(), Some(true));
}

#[test]
fn failed_requirement_propagates_through_minimums() {
    // El laboratorio tiene promedio 4,5 pero reprueba por un informe < 3,0; como el
    // laboratorio debe aprobarse para el ramo, el ramo también queda reprobado.
    let mut tree = Tree::new();
    let course = tree.create(None, "Programación").unwrap();
    leaf(&mut tree, course, "Teoría", Some(6.0));
    let lab = tree.create(Some(course), "Laboratorio").unwrap();
    let report = leaf(&mut tree, lab, "Informe", Some(2.0));
    leaf(&mut tree, lab, "Proyecto", Some(7.0));
    tree.set_rules(report, minimum(3.0, None)).unwrap();

    let eval = tree.evaluate();
    assert!(!eval.get(lab).unwrap().requirements_met);
    assert!(eval.get(course).unwrap().requirements_met);

    tree.set_rules(lab, minimum(4.0, None)).unwrap();
    assert!(!tree.evaluate().get(course).unwrap().requirements_met);
}

// ------------------------------------------------------------------ créditos

#[test]
fn average_by_credits() {
    let mut tree = Tree::new();
    let year = tree.create(None, "Primer año").unwrap();
    let s1 = tree.create(Some(year), "Semestre 1").unwrap();
    let a = leaf(&mut tree, s1, "Cálculo", Some(6.0));
    let b = leaf(&mut tree, s1, "Física", Some(4.0));
    tree.set_credits(a, Some(10.0)).unwrap();
    tree.set_credits(b, Some(5.0)).unwrap();

    // Apagado: promedio simple.
    close(tree.evaluate().grade(s1), 5.0);

    tree.set_aggregation(s1, Aggregation::Credits).unwrap();
    let eval = tree.evaluate();
    close(eval.grade(s1), 80.0 / 15.0);
    assert!(eval.warnings().is_empty());
    close(Some(eval.get(a).unwrap().weight), 200.0 / 3.0);

    // El año pondera por créditos de cada semestre (suma de sus ramos).
    let s2 = tree.create(Some(year), "Semestre 2").unwrap();
    let c = leaf(&mut tree, s2, "Química", Some(7.0));
    tree.set_credits(c, Some(5.0)).unwrap();
    tree.set_aggregation(year, Aggregation::Credits).unwrap();
    assert_eq!(tree.effective_credits(s1), Some(15.0));
    close(tree.evaluate().grade(year), (80.0 + 35.0) / 20.0);

    // Un ramo sin créditos cuenta 0, con aviso.
    tree.set_aggregation(s2, Aggregation::Credits).unwrap();
    let d = leaf(&mut tree, s2, "Inglés", Some(1.0));
    let eval = tree.evaluate();
    close(eval.grade(s2), 7.0);
    assert!(
        eval.warnings_for(d)
            .any(|w| matches!(w, Warning::MissingCredits { .. }))
    );
}

// ------------------------------------------------------------------ con la nota mínima

#[test]
fn needed_grade_respects_drop_lowest() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Cálculo").unwrap();
    leaf(&mut tree, course, "C1", Some(2.0));
    leaf(&mut tree, course, "C2", Some(5.0));
    leaf(&mut tree, course, "C3", None);
    tree.set_round_result(course, Some(false)).unwrap();
    tree.set_target(course, Some(4.0)).unwrap();

    // (2 + 5 + x) / 3 ≥ 4 → 5,0.
    close(Some(needed(&tree, course)), 5.0);
    // Se elimina el 2,0: (5 + x) / 2 ≥ 4 → 3,0.
    tree.set_rules(course, drop_lowest(1)).unwrap();
    close(Some(needed(&tree, course)), 3.0);
}

#[test]
fn needed_grade_respects_replacement() {
    // El examen reemplaza la peor y además cuenta: (max(2, x) + 5 + x) / 3 ≥ 4 → x ≥ 3,5.
    let mut tree = Tree::new();
    let course = tree.create(None, "Química").unwrap();
    leaf(&mut tree, course, "C1", Some(2.0));
    leaf(&mut tree, course, "C2", Some(5.0));
    let exam = leaf(&mut tree, course, "Examen", None);
    tree.set_round_result(course, Some(false)).unwrap();
    tree.set_target(course, Some(4.0)).unwrap();
    let mut rules = replaces(ReplaceTarget::Lowest);
    rules.replaces.as_mut().unwrap().counts_itself = true;
    tree.set_rules(exam, rules).unwrap();
    close(Some(needed(&tree, course)), 3.5);
}

#[test]
fn needed_grade_respects_exemption() {
    // C1 6,0; C2 y examen sin nota; se exime con 5,5. Para un 5,5 final (aproximando):
    // sin eximirse no se puede (haría falta x ≥ 5,175 y eso ya exime); con C2 ≥ 5,0 se exime
    // y (6 + 5) / 2 = 5,5. El examen no aparece en lo que falta.
    let mut tree = Tree::new();
    let course = tree.create(None, "Biología").unwrap();
    leaf(&mut tree, course, "C1", Some(6.0));
    let c2 = leaf(&mut tree, course, "C2", None);
    let exam = leaf(&mut tree, course, "Examen", None);
    tree.set_rules(exam, exemption(5.5, false)).unwrap();
    tree.set_target(course, Some(5.5)).unwrap();

    match tree.requirement(course).unwrap() {
        Some(Requirement::Needed { grade, missing }) => {
            close(Some(grade), 5.0);
            assert_eq!(missing.len(), 1);
            assert_eq!(missing[0].node, c2);
        }
        other => panic!("{other:?}"),
    }

    // Objetivo 4,0: con 3,0 en ambas no se exime pero (6 + 3 + 3) / 3 = 4,0.
    tree.set_target(course, Some(4.0)).unwrap();
    match tree.requirement(course).unwrap() {
        Some(Requirement::Needed { grade, missing }) => {
            close(Some(grade), 3.0);
            assert_eq!(missing.len(), 2);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn needed_grade_respects_minimum() {
    // (6 + x) / 2 ≥ 4 → 2,0, pero el laboratorio debe tener ≥ 4,0.
    let mut tree = Tree::new();
    let course = tree.create(None, "Programación").unwrap();
    leaf(&mut tree, course, "Teoría", Some(6.0));
    let lab = leaf(&mut tree, course, "Laboratorio", None);
    tree.set_round_result(course, Some(false)).unwrap();
    tree.set_target(course, Some(4.0)).unwrap();

    close(Some(needed(&tree, course)), 2.0);
    tree.set_rules(lab, minimum(4.0, None)).unwrap();
    close(Some(needed(&tree, course)), 4.0);

    // Ya con nota bajo el mínimo: no alcanzable aunque el promedio dé.
    tree.set_grade(lab, Some(3.0)).unwrap();
    assert_eq!(
        tree.requirement(course).unwrap(),
        Some(Requirement::Unreachable)
    );
}

#[test]
fn needed_grade_respects_credits() {
    // Semestre por créditos: A (10 cr) 3,0 y B (5 cr) sin nota. (30 + 5x) / 15 ≥ 4 → x ≥ 6,0.
    let mut tree = Tree::new();
    let semester = tree.create(None, "Semestre").unwrap();
    tree.set_aggregation(semester, Aggregation::Credits)
        .unwrap();
    let a = leaf(&mut tree, semester, "A", Some(3.0));
    let b = leaf(&mut tree, semester, "B", None);
    tree.set_credits(a, Some(10.0)).unwrap();
    tree.set_credits(b, Some(5.0)).unwrap();
    tree.set_round_result(semester, Some(false)).unwrap();
    tree.set_target(semester, Some(4.0)).unwrap();
    close(Some(needed(&tree, semester)), 6.0);
}
