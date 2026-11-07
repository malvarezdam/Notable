use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

/// Cómo se reduce una nota a la cantidad de decimales configurada.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rounding {
    /// Aproximar al más cercano (4,45 → 4,5 con 1 decimal).
    #[default]
    Round,
    /// Truncar (4,49 → 4,4 con 1 decimal).
    Truncate,
}

/// Cómo se combinan las notas de los hijos de un nodo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aggregation {
    /// Promedio simple (por defecto).
    #[default]
    Simple,
    /// Promedio ponderado con los pesos de cada hijo (botón "Ponderar").
    Weighted,
    /// Promedio ponderado por los créditos de cada hijo (semestre, año, carrera).
    Credits,
}

/// Cómo se pasa una nota de esta escala a la escala del padre, cuando son distintas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conversion {
    /// Proporcional: el mínimo va al mínimo y el máximo al máximo (por defecto).
    #[default]
    Linear,
    /// En dos tramos: además, la nota de aprobación va a la nota de aprobación del padre
    /// (por ejemplo 55 de 100 → 4,0).
    ByPass,
}

/// Configuración de nota de un nodo. Es heredable: un nodo sin configuración propia usa la de su
/// ancestro más cercano que tenga una (ver [`crate::Tree::effective_config`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GradeConfig {
    /// Nota mínima de la escala.
    pub min: f64,
    /// Nota máxima de la escala.
    pub max: f64,
    /// Nota con la que se aprueba.
    pub pass: f64,
    /// Decimales con que se muestra y redondea la nota.
    pub decimals: u8,
    /// Aproximar o truncar.
    pub rounding: Rounding,
    /// Redondear el promedio de cada nivel antes de usarlo en el de arriba (apagado por defecto:
    /// solo se redondean los nodos que lo piden con [`crate::Node::round_result`]).
    pub round_every_level: bool,
    /// Cómo se convierte a la escala del padre, si es distinta.
    pub conversion: Conversion,
}

impl GradeConfig {
    /// Máximo de decimales permitido.
    pub const MAX_DECIMALS: u8 = 6;

    /// Escala 1,0–7,0, aprueba con 4,0, un decimal, aproximando.
    pub const SCALE_1_7: Self = Self {
        min: 1.0,
        max: 7.0,
        pass: 4.0,
        decimals: 1,
        rounding: Rounding::Round,
        round_every_level: false,
        conversion: Conversion::Linear,
    };

    /// Escala 0–100, aprueba con 55, sin decimales, aproximando.
    pub const SCALE_0_100: Self = Self {
        min: 0.0,
        max: 100.0,
        pass: 55.0,
        decimals: 0,
        rounding: Rounding::Round,
        round_every_level: false,
        conversion: Conversion::Linear,
    };

    /// Crea una configuración validada (sin redondeo por nivel y con conversión lineal).
    pub fn new(
        min: f64,
        max: f64,
        pass: f64,
        decimals: u8,
        rounding: Rounding,
    ) -> Result<Self, ConfigError> {
        let config = Self {
            min,
            max,
            pass,
            decimals,
            rounding,
            ..Self::default()
        };
        config.validate()?;
        Ok(config)
    }

    /// Comprueba que la escala tenga sentido.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.min.is_finite() || !self.max.is_finite() || !self.pass.is_finite() {
            return Err(ConfigError::NotFinite);
        }
        if self.min >= self.max {
            return Err(ConfigError::EmptyScale);
        }
        if self.pass < self.min || self.pass > self.max {
            return Err(ConfigError::PassOutOfScale);
        }
        if self.decimals > Self::MAX_DECIMALS {
            return Err(ConfigError::TooManyDecimals);
        }
        Ok(())
    }

    /// `true` si `grade` está dentro de la escala.
    pub fn contains(&self, grade: f64) -> bool {
        grade >= self.min && grade <= self.max
    }

    /// Lleva `grade` a los decimales configurados, aproximando o truncando.
    ///
    /// Antes se limpia el error de punto flotante (4,45 se guarda como 4,4499999…), para que
    /// 4,45 aproxime a 4,5 y 4,3 no se trunque a 4,2.
    pub fn apply_rounding(&self, grade: f64) -> f64 {
        let factor = 10f64.powi(i32::from(self.decimals));
        // 9 cifras significativas extra bastan para notas y quitan el ruido binario.
        let scaled = (grade * factor * 1e9).round() / 1e9;
        let reduced = match self.rounding {
            Rounding::Round => scaled.round(),
            Rounding::Truncate => scaled.trunc(),
        };
        reduced / factor
    }

    /// Pasa `grade`, que está en esta escala, a la escala `target`.
    /// Usa la [`Conversion`] de esta configuración.
    pub fn convert_to(&self, grade: f64, target: &GradeConfig) -> f64 {
        match self.conversion {
            Conversion::Linear => {
                if self.min == target.min && self.max == target.max {
                    grade
                } else {
                    lerp(grade, self.min, self.max, target.min, target.max)
                }
            }
            Conversion::ByPass => {
                if self.min == target.min && self.max == target.max && self.pass == target.pass {
                    grade
                } else if grade < self.pass {
                    lerp(grade, self.min, self.pass, target.min, target.pass)
                } else {
                    lerp(grade, self.pass, self.max, target.pass, target.max)
                }
            }
        }
    }
}

/// La configuración que se usa cuando ningún ancestro define una: escala 1,0–7,0.
impl Default for GradeConfig {
    fn default() -> Self {
        Self::SCALE_1_7
    }
}

/// Lleva `x` del tramo `[a0, a1]` al tramo `[b0, b1]` (extrapola fuera del tramo).
fn lerp(x: f64, a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    if a1 == a0 {
        // Tramo vacío (aprobación en el extremo de la escala): todo cae en el borde.
        return if x < a0 { b0 } else { b1 };
    }
    b0 + (x - a0) * (b1 - b0) / (a1 - a0)
}
