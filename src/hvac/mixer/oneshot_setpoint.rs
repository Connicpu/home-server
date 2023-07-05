use serde::{Deserialize, Serialize};
use std::sync::RwLock;

use super::HvacRequest;

pub struct OneshotSetpoint {
    state: RwLock<Option<OneshotSetpointState>>,
}

impl OneshotSetpoint {
    pub fn new() -> Self {
        OneshotSetpoint {
            state: Default::default(),
        }
    }

    pub fn get(&self) -> Option<OneshotSetpointState> {
        self.state.read().unwrap().clone()
    }

    pub fn set(&self, state: Option<OneshotSetpointState>) {
        *self.state.write().unwrap() = state;
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum OneshotOrdering {
    #[serde(rename = "less")]
    Less,
    #[serde(rename = "greater")]
    Greater,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct OneshotSetpointState {
    /// Degrees Celcius
    pub setpoint: f32,
    pub comparison: OneshotOrdering,
    pub action: HvacRequest,
}


