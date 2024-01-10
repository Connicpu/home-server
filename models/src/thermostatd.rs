use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

use crate::hvac_request::HvacRequest;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct TimedOverride {
    pub command: HvacRequest,
    pub expiration: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OneshotOverride {
    pub command: HvacRequest,
    pub comparison: OneshotOrdering,
    pub setpoint: f32,
    pub probe: String,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum OneshotOrdering {
    #[serde(rename = "less")]
    Less,
    #[serde(rename = "greater")]
    Greater,
}
