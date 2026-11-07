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
    };

    /// Escala 0–100, aprueba con 55, sin decimales, aproximando.
    pub const SCALE_0_100: Self = Self {
        min: 0.0,
        max: 100.0,
        pass: 55.0,
        decimals: 0,
        rounding: Rounding::Round,
    };

    /// Crea una configuración validada.
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
}

/// La configuración que se usa cuando ningún ancestro define una: escala 1,0–7,0.
impl Default for GradeConfig {
    fn default() -> Self {
        Self::SCALE_1_7
    }
}
