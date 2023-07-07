use std::fmt;

use serde::{Serialize, Deserialize};

pub use models::hvac_request::HvacRequest;

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Units {
    Celcius,
    Fahrenheit,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HvacMode {
    Off,
    Heat,
    Cool,
}

impl fmt::Display for HvacMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            HvacMode::Off => f.write_str("Off"),
            HvacMode::Heat => f.write_str("Heat"),
            HvacMode::Cool => f.write_str("Cool"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HvacModeState {
    pub mode: HvacMode,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum OneshotOrdering {
    #[serde(rename = "less")]
    Less,
    #[serde(rename = "greater")]
    Greater,
}

#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct OneshotSetpointState {
    /// Degrees Celcius
    pub setpoint: f32,
    pub comparison: OneshotOrdering,
    pub action: HvacRequest,
}

#[derive(Copy, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Temperature(pub f32);

#[derive(Copy, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PinState(pub HvacRequest);


