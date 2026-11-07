//! Tipos que la app (Tauri) le entrega a la interfaz (Leptos).
//!
//! Son datos ya listos para mostrar: la interfaz no calcula nada. Los ids van como texto para
//! no arrastrar `uuid` a WebAssembly.

use serde::{Deserialize, Serialize};

/// Lo que se ve en pantalla al estar parado en un nodo (o en el inicio).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Screen {
    /// Migas de pan: del nivel superior hasta el nodo actual (incluido). Vacío = inicio.
    pub path: Vec<Crumb>,
    /// El nodo actual. `None` = inicio (lista de nodos de nivel superior).
    pub current: Option<Item>,
    /// Hijos visibles del nodo actual, en orden.
    pub children: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Crumb {
    pub id: String,
    pub name: String,
}

/// Un nodo listo para mostrar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub name: String,
    /// Sin hijos: su nota se ingresa.
    pub is_leaf: bool,
    /// Nota ingresada (solo hojas).
    pub grade: Option<f64>,
    /// Nota del nodo (ingresada o calculada), sin formato.
    pub value: Option<f64>,
    /// Nota formateada para mostrar ("5,3"), o "—" si todavía no hay.
    pub display: String,
    /// Todas las notas que cuentan están ingresadas.
    pub complete: bool,
    /// Aprobado (`None` si no hay nota).
    pub passed: Option<bool>,
    /// Cantidad de hijos visibles.
    pub children: usize,
    pub scale: Scale,
}

/// La escala que usa un nodo (para validar y mostrar la nota).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scale {
    pub min: f64,
    pub max: f64,
    pub decimals: u8,
}

impl Scale {
    pub fn contains(&self, grade: f64) -> bool {
        grade >= self.min && grade <= self.max
    }
}

/// Formatea una nota con `decimals` decimales y coma decimal ("4,5"). La nota ya debe venir
/// redondeada o truncada según su configuración.
pub fn format_grade(grade: f64, decimals: u8) -> String {
    format!("{grade:.*}", usize::from(decimals)).replace('.', ",")
}

/// Lee una nota escrita por el usuario ("4,5", "4.5", " 60 "). Vacío = sin nota.
pub fn parse_grade(text: &str) -> Result<Option<f64>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    text.replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|g| g.is_finite())
        .map(Some)
        .ok_or_else(|| format!("«{text}» no es una nota"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_and_parse() {
        assert_eq!(format_grade(5.0, 1), "5,0");
        assert_eq!(format_grade(4.25, 2), "4,25");
        assert_eq!(format_grade(60.0, 0), "60");
        assert_eq!(parse_grade(" 4,5 "), Ok(Some(4.5)));
        assert_eq!(parse_grade("4.5"), Ok(Some(4.5)));
        assert_eq!(parse_grade(""), Ok(None));
        assert!(parse_grade("abc").is_err());
        assert!(parse_grade("inf").is_err());
    }
}
