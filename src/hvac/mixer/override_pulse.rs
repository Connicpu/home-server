use std::sync::RwLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::HvacRequest;

pub struct OverridePulse {
    state: RwLock<Option<OverridePulseState>>,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct OverridePulseState {
    active_until: DateTime<Utc>,
    request: HvacRequest,
}

impl OverridePulse {
    pub fn new() -> Self {
        OverridePulse {
            state: RwLock::new(None),
        }
    }

    pub fn evaluate(&self) -> Option<HvacRequest> {
        let current = self.get();
        if let Some(current) = current {
            if current.active_until > Utc::now() {
                return Some(current.request);
            }
        }
        None
    }

    pub fn get(&self) -> Option<OverridePulseState> {
        *self.state.read().unwrap()
    }

    pub fn set(&self, state: Option<OverridePulseState>) {
        *self.state.write().unwrap() = state;
    }
}

impl Default for OverridePulse {
    fn default() -> Self {
        Self::new()
    }
}
