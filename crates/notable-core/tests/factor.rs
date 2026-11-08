//! Regla opcional de factor: una nota (por ejemplo, el laboratorio) que no se promedia sino que
//! multiplica la nota del padre.

use notable_core::{
    Aggregation, Factor, Node, NodeId, Requirement, Rules, Tree, TreeError, Warning,
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

fn factor(min: f64, max: f64, neutral: Option<f64>) -> Rules {
    Rules {
        factor: Some(Factor { min, max, neutral }),
        ..Rules::default()
    }
}

/// Ramo con dos certámenes (5,0 y 6,0 → promedio 5,5) y un laboratorio que es factor.
fn physics(lab: Option<f64>, rule: Rules) -> (Tree, NodeId, NodeId) {
    let mut tree = Tree::new();
    let course = tree.create(None, "Física").unwrap();
    leaf(&mut tree, course, "C1", Some(5.0));
    leaf(&mut tree, course, "C2", Some(6.0));
    let lab = leaf(&mut tree, course, "Laboratorio", lab);
    tree.set_rules(lab, rule).unwrap();
    (tree, course, lab)
}

#[test]
fn factor_is_proportional_over_the_scale() {
    // 1,0 → 0,8 y 7,0 → 1,1.
    for (lab, expected_factor) in [(7.0, 1.1), (1.0, 0.8), (4.0, 0.95)] {
        let (tree, course, lab_id) = physics(Some(lab), factor(0.8, 1.1, None));
        let eval = tree.evaluate();
        close(eval.get(lab_id).unwrap().factor, expected_factor);
        close(eval.grade(course), 5.5 * expected_factor);
        assert!(eval.get(course).unwrap().complete);
    }
}

#[test]
fn neutral_grade_gives_factor_one() {
    // 4,0 → 1; bajo 4,0 va de 0,8 a 1 y sobre 4,0 de 1 a 1,1.
    for (lab, expected_factor) in [(4.0, 1.0), (1.0, 0.8), (5.5, 1.05), (7.0, 1.1)] {
        let (tree, course, _) = physics(Some(lab), factor(0.8, 1.1, Some(4.0)));
        close(tree.evaluate().grade(course), 5.5 * expected_factor);
    }
}

#[test]
fn result_stays_within_the_scale() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Ramo").unwrap();
    leaf(&mut tree, course, "C1", Some(6.8));
    let lab = leaf(&mut tree, course, "Lab", Some(7.0));
    tree.set_rules(lab, factor(0.8, 1.2, None)).unwrap();
    close(tree.evaluate().grade(course), 7.0);
}

#[test]
fn factor_is_not_averaged_nor_weighted() {
    let mut tree = Tree::new();
    let course = tree.create(None, "Ramo").unwrap();
    tree.set_aggregation(course, Aggregation::Weighted).unwrap();
    let c1 = leaf(&mut tree, course, "C1", Some(4.0));
    let c2 = leaf(&mut tree, course, "C2", Some(6.0));
    tree.set_weight(c1, Some(60.0)).unwrap();
    tree.set_weight(c2, Some(40.0)).unwrap();
    // Sin peso: no debería avisar que falta ni que los pesos no suman 100 %.
    let lab = leaf(&mut tree, course, "Lab", Some(7.0));
    tree.set_rules(lab, factor(1.0, 1.1, None)).unwrap();

    let eval = tree.evaluate();
    close(eval.grade(course), 4.8 * 1.1);
    assert!(eval.warnings().is_empty(), "{:?}", eval.warnings());

    // En promedio simple tampoco entra: (4 + 6) / 2 = 5.
    tree.set_aggregation(course, Aggregation::Simple).unwrap();
    close(tree.evaluate().grade(course), 5.0 * 1.1);
}

#[test]
fn missing_factor_leaves_the_grade_partial() {
    let (tree, course, lab) = physics(None, factor(0.8, 1.1, None));
    let eval = tree.evaluate();
    close(eval.grade(course), 5.5);
    assert!(!eval.get(course).unwrap().complete);
    assert_eq!(eval.get(lab).unwrap().factor, None);
}

#[test]
fn factor_with_a_minimum_requirement() {
    let mut rules = factor(0.8, 1.1, None);
    rules.minimum = Some(notable_core::Minimum {
        grade: 4.0,
        cap: Some(3.9),
    });
    let (tree, course, lab) = physics(Some(3.0), rules);
    let eval = tree.evaluate();
    let result = eval.get(course).unwrap();
    assert_eq!(result.passed(), Some(false));
    close(result.grade, 3.9);
    assert!(
        eval.warnings()
            .iter()
            .any(|w| matches!(w, Warning::MinimumNotMet { node, .. } if *node == lab))
    );
}

#[test]
fn invalid_factors_are_rejected() {
    let (mut tree, _, lab) = physics(Some(5.0), Rules::default());
    for bad in [
        factor(1.2, 0.8, None),           // mínimo sobre el máximo
        factor(-0.1, 1.0, None),          // negativo
        factor(1.05, 1.2, Some(4.0)),     // con nota neutra, 1 debe quedar en el rango
        factor(0.8, f64::INFINITY, None), // no finito
    ] {
        assert_eq!(tree.set_rules(lab, bad), Err(TreeError::InvalidRule));
    }
    let mut with_exemption = factor(0.8, 1.1, None);
    with_exemption.exemption = Some(notable_core::Exemption::default());
    assert_eq!(
        tree.set_rules(lab, with_exemption),
        Err(TreeError::InvalidRule)
    );
}

#[test]
fn needed_grade_accounts_for_the_factor() {
    // Con laboratorio 7,0 (× 1,1) basta un 4,0 en C2: (5 + 4) / 2 × 1,1 = 4,95 → 5,0.
    let mut tree = Tree::new();
    let course = tree.create(None, "Ramo").unwrap();
    leaf(&mut tree, course, "C1", Some(5.0));
    let c2 = leaf(&mut tree, course, "C2", None);
    let lab = leaf(&mut tree, course, "Lab", Some(7.0));
    tree.set_rules(lab, factor(0.8, 1.1, None)).unwrap();
    tree.set_target(course, Some(5.0)).unwrap();
    match tree.requirement(course).unwrap() {
        Some(Requirement::Needed { grade, missing }) => {
            close(Some(grade), 4.0);
            assert_eq!(missing[0].node, c2);
        }
        other => panic!("esperaba Needed, salió {other:?}"),
    }

    // Si lo que falta es el laboratorio: 5,0 × f ≥ 4,95 → f ≥ 0,99 → laboratorio 4,8.
    let (mut tree, course, lab) = physics(None, factor(0.8, 1.1, None));
    let c2 = tree.children(Some(course)).nth(1).unwrap().id;
    tree.set_grade(c2, Some(5.0)).unwrap();
    tree.set_target(course, Some(5.0)).unwrap();
    match tree.requirement(course).unwrap() {
        Some(Requirement::Needed { grade, missing }) => {
            close(Some(grade), 4.8);
            assert_eq!(missing[0].node, lab);
        }
        other => panic!("esperaba Needed, salió {other:?}"),
    }
}

#[test]
fn factor_is_saved_and_old_data_still_loads() {
    let mut node = Node::new("Lab", None);
    node.rules = factor(0.8, 1.1, Some(4.0));
    let json = node.to_json().unwrap();
    assert!(json.contains("\"factor\""));
    assert_eq!(Node::from_json(&json).unwrap(), node);

    // Sin la regla no se escribe nada (datos viejos y nuevos son iguales).
    let plain = Node::new("C1", None);
    assert!(!plain.to_json().unwrap().contains("factor"));
}
