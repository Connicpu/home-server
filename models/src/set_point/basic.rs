use serde::{Deserialize, Serialize};

use crate::mixer::Mixer;

use super::EMPTY_REQUEST;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct BasicSetPoint {
    pub probe: String,
    pub weight: f32,
    pub min_temp: f32,
    pub max_temp: f32,
}

impl BasicSetPoint {
    pub async fn evaluate(&self, state: &impl Mixer) -> (f32, f32) {
        let Some(temp) = state.get_probe_temp(&self.probe).await else {
            return EMPTY_REQUEST;
        };

        if temp < self.min_temp {
            let diff = self.min_temp - temp;
            (diff * self.weight, 0.0)
        } else if temp > self.max_temp {
            let diff = temp - self.max_temp;
            (0.0, diff * self.weight)
        } else {
            EMPTY_REQUEST
        }
    }
}
