use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

use arc_cell::ArcCell;
use serde::{Serialize, Deserialize};

use crate::RedisConn;

use self::{override_pulse::OverridePulse, timed_rule::TimedRuleSet};

use super::Probes;

pub mod override_pulse;
pub mod set_point;
pub mod timed_rule;

#[derive(Clone)]
pub struct MixerState {
    pub redis: RedisConn,
    pub probes: Probes,
    pub override_pulse: Arc<OverridePulse>,
    pub timed_ruleset: Arc<TimedRuleSet>,
    pub last_result: Arc<AtomicHvacRequest>,
    pub mode: Arc<AtomicHvacRequest>,
}

impl MixerState {
    pub async fn new(redis: &RedisConn, probes: Probes, mode: Arc<AtomicHvacRequest>) -> Arc<Self> {
        Arc::new(MixerState {
            redis: redis.clone(),
            probes,
            override_pulse: Arc::new(OverridePulse::new()),
            timed_ruleset: Arc::new(TimedRuleSet::load(redis).await),
            last_result: Arc::new(AtomicHvacRequest::new()),
            mode,
        })
    }

    pub async fn query(&self) -> HvacRequest {
        if let Some(request) = self.get_query().await {
            self.last_result.store(request);
            request
        } else {
            self.last_result.load()
        }
    }

    async fn get_query(&self) -> Option<HvacRequest> {
        // Check if there's an override pulse
        if let Some(request) = self.override_pulse.evaluate() {
            return Some(request);
        }

        // Evaluate our rule based setpoints
        if let Some(request) = self.timed_ruleset.evaluate(self).await {
            return Some(request);
        }

        None
    }

    pub fn mode(&self) -> HvacRequest {
        self.mode.load()
    }
}

#[derive(Clone)]
pub struct Mixer {
    state: ArcCell<MixerState>,
}

impl Mixer {
    pub fn new(state: Arc<MixerState>) -> Self {
        Mixer {
            state: ArcCell::new(state),
        }
    }

    pub async fn query(&self) -> HvacRequest {
        self.state().query().await
    }

    pub fn state(&self) -> Arc<MixerState> {
        self.state.get()
    }

    pub fn replace_state(&self, mixer: Arc<MixerState>) {
        self.state.set(mixer);
    }

    pub fn update_state(&self, update: impl FnOnce(&mut MixerState)) {
        let mut state = self.state();
        update(Arc::make_mut(&mut state));
        self.replace_state(state);
    }

    pub async fn reload_timed_rules(&self) {
        let new_rules = Arc::new(TimedRuleSet::load(&self.state().redis).await);
        self.update_state(|state| state.timed_ruleset = new_rules);
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HvacRequest {
    Off,
    Heat,
    Cool,
}

impl HvacRequest {
    pub fn from_payload(payload: &[u8]) -> Option<HvacRequest> {
        match payload.get(0) {
            Some(b'o') => Some(HvacRequest::Off),
            Some(b'h') => Some(HvacRequest::Heat),
            Some(b'c') => Some(HvacRequest::Cool),
            _ => None,
        }
    }

    pub fn payload_str(self) -> &'static str {
        match self {
            HvacRequest::Off => "off",
            HvacRequest::Heat => "heat",
            HvacRequest::Cool => "cool",
        }
    }

    pub fn payload(self) -> &'static [u8] {
        self.payload_str().as_bytes()
    }
}

impl std::ops::BitAnd for HvacRequest {
    type Output = HvacRequest;
    fn bitand(self, rhs: HvacRequest) -> HvacRequest {
        match (self, rhs) {
            (HvacRequest::Heat, HvacRequest::Heat) => HvacRequest::Heat,
            (HvacRequest::Cool, HvacRequest::Cool) => HvacRequest::Cool,
            _ => HvacRequest::Off,
        }
    }
}

#[derive(Default)]
pub struct AtomicHvacRequest(AtomicU8);

impl AtomicHvacRequest {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn store(&self, request: HvacRequest) {
        let value = match request {
            HvacRequest::Off => 0,
            HvacRequest::Heat => 1,
            HvacRequest::Cool => 2,
        };
        self.0.store(value, Ordering::SeqCst);
    }

    pub fn load(&self) -> HvacRequest {
        match self.0.load(Ordering::SeqCst) {
            1 => HvacRequest::Heat,
            2 => HvacRequest::Cool,
            _ => HvacRequest::Off,
        }
    }
}
