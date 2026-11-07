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
    /// Ajustes del nodo actual (para el panel de ajustes). `None` en el inicio.
    pub details: Option<Details>,
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
    /// Peso ingresado (en %), respecto a `weight_base` o al padre.
    pub weight: Option<f64>,
    /// Id del ancestro respecto al cual se expresa el peso. `None` = respecto al padre.
    pub weight_base: Option<String>,
    /// Lo que vale dentro del padre cuando el peso se expresó respecto a otro nivel
    /// ("= 25 % de Tareas").
    pub weight_note: Option<String>,
    /// Créditos propios.
    pub credits: Option<f64>,
    /// Qué le hizo una regla opcional ("Eliminada", "Eximida"…).
    pub effect: Option<String>,
    /// Reglas activas en este nodo, en corto ("Mínimo 4,0").
    pub badges: Vec<String>,
    /// Avisos de este nodo.
    pub warnings: Vec<String>,
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

/// Cómo se combinan los hijos de un nodo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Simple,
    Weighted,
    Credits,
}

/// Configuración de nota editable.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScaleSettings {
    pub min: f64,
    pub max: f64,
    pub pass: f64,
    pub decimals: u8,
    /// Truncar en vez de aproximar.
    pub truncate: bool,
    /// Redondear el promedio de cada nivel.
    pub round_every_level: bool,
    /// Convertir a la escala del padre haciendo calzar la nota de aprobación.
    pub by_pass: bool,
}

impl ScaleSettings {
    /// Escala 1,0–7,0, aprueba con 4,0.
    pub const SCALE_1_7: Self = Self {
        min: 1.0,
        max: 7.0,
        pass: 4.0,
        decimals: 1,
        truncate: false,
        round_every_level: false,
        by_pass: false,
    };

    /// Escala 0–100, aprueba con 55.
    pub const SCALE_0_100: Self = Self {
        min: 0.0,
        max: 100.0,
        pass: 55.0,
        decimals: 0,
        truncate: false,
        round_every_level: false,
        by_pass: false,
    };
}

/// Configuración de nota del nodo actual.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConfigView {
    /// El nodo tiene configuración propia.
    pub own: bool,
    /// Si es heredada: nombre del nodo de donde viene. `None` = la configuración por defecto.
    pub inherited_from: Option<String>,
    pub settings: ScaleSettings,
}

/// Ajustes del nodo actual.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Details {
    pub mode: Mode,
    pub config: ConfigView,
    /// `Some(true)`/`Some(false)` = forzado; `None` = automático.
    pub round_result: Option<bool>,
    pub target: Option<f64>,
    /// Tarjeta "Necesitas X en …" (solo si hay objetivo).
    pub goal: Option<Goal>,
    pub credits: Option<f64>,
    /// Créditos que suman sus hijos (cuando no tiene propios).
    pub child_credits: Option<f64>,
    pub rules: RulesSettings,
    /// Hermanos (para elegir a quién reemplaza).
    pub siblings: Vec<Crumb>,
    /// Tiene padre (las reglas de reemplazo, eximición y mínimo actúan en el padre).
    pub has_parent: bool,
    /// Avisos del nodo actual.
    pub warnings: Vec<String>,
}

/// Resultado de la nota mínima necesaria, listo para mostrar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub kind: GoalKind,
    /// Titular: "Necesitas 4,3", "Objetivo asegurado", "No alcanzable".
    pub title: String,
    /// Detalle: "en Certamen 2", "en cada una de: …".
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalKind {
    Needed,
    Met,
    Unreachable,
}

/// Reglas opcionales de un nodo (todas apagadas por defecto).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RulesSettings {
    /// Eliminar las N peores notas de los hijos (0 = no).
    pub drop_lowest: u32,
    pub replaces: Option<ReplaceSettings>,
    pub exemption: Option<ExemptionSettings>,
    pub minimum: Option<MinimumSettings>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReplaceSettings {
    /// Id del hermano que reemplaza. `None` = la peor nota.
    pub target: Option<String>,
    pub only_if_better: bool,
    pub counts_itself: bool,
}

impl Default for ReplaceSettings {
    fn default() -> Self {
        Self {
            target: None,
            only_if_better: true,
            counts_itself: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExemptionSettings {
    pub threshold: f64,
    pub keep_if_better: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MinimumSettings {
    pub grade: f64,
    pub cap: Option<f64>,
}

/// Un cambio en un nodo (comando `update`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Change {
    Mode(Mode),
    Weight(Option<f64>),
    /// Id del ancestro; `None` = respecto al padre.
    WeightBase(Option<String>),
    /// `None` = heredar.
    Config(Option<ScaleSettings>),
    Target(Option<f64>),
    RoundResult(Option<bool>),
    Credits(Option<f64>),
    Rules(RulesSettings),
}

/// Formatea una nota con `decimals` decimales y coma decimal ("4,5"). La nota ya debe venir
/// redondeada o truncada según su configuración.
pub fn format_grade(grade: f64, decimals: u8) -> String {
    format!("{grade:.*}", usize::from(decimals)).replace('.', ",")
}

/// Formatea un número sin ceros de más ("25", "12,5", "33,33").
pub fn format_number(value: f64) -> String {
    let text = format!("{value:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.replace('.', ",")
}

/// Lee una nota escrita por el usuario ("4,5", "4.5", " 60 "). Vacío = sin nota.
pub fn parse_grade(text: &str) -> Result<Option<f64>, String> {
    let text = text.trim().trim_end_matches('%').trim();
    if text.is_empty() {
        return Ok(None);
    }
    text.replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|g| g.is_finite())
        .map(Some)
        .ok_or_else(|| format!("«{text}» no es un número"))
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
        assert_eq!(parse_grade("30 %"), Ok(Some(30.0)));
        assert_eq!(parse_grade(""), Ok(None));
        assert!(parse_grade("abc").is_err());
        assert!(parse_grade("inf").is_err());
    }

    #[test]
    fn numbers_without_trailing_zeros() {
        assert_eq!(format_number(25.0), "25");
        assert_eq!(format_number(12.5), "12,5");
        assert_eq!(format_number(100.0 / 3.0), "33,33");
        assert_eq!(format_number(0.0), "0");
    }
}
