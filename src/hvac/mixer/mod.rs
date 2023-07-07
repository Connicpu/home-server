use std::{
    cmp,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};

use arc_cell::ArcCell;

use crate::{api::atticfan::FanState, RedisConn};

use self::{
    oneshot_setpoint::{OneshotOrdering, OneshotSetpoint},
    override_pulse::OverridePulse,
    timed_rule::TimedRuleSet,
};

use super::Probes;

pub use models::hvac_request::HvacRequest;

pub mod oneshot_setpoint;
pub mod override_pulse;
pub mod set_point;
pub mod timed_rule;

#[derive(Clone)]
pub struct MixerState {
    pub redis: RedisConn,
    pub probes: Probes,
    pub fan_state: FanState,
    pub override_pulse: Arc<OverridePulse>,
    pub oneshot_setpoint: Arc<OneshotSetpoint>,
    pub timed_ruleset: Arc<TimedRuleSet>,
    pub last_result: Arc<AtomicHvacRequest>,
    pub mode: Arc<AtomicHvacRequest>,
}

impl MixerState {
    pub async fn new(
        redis: &RedisConn,
        probes: Probes,
        mode: Arc<AtomicHvacRequest>,
        fan_state: FanState,
    ) -> Arc<Self> {
        Arc::new(MixerState {
            redis: redis.clone(),
            probes,
            fan_state,
            override_pulse: Arc::new(OverridePulse::new()),
            oneshot_setpoint: Arc::new(OneshotSetpoint::new()),
            timed_ruleset: Arc::new(timed_rule::load(redis).await),
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

        // Check if the big succ is running
        if self.fan_state.big_succ().await {
            return Some(HvacRequest::Off);
        }

        // Execute a oneshot setpoint if it exists
        'oneshot: {
            let Some(setpoint) = self.oneshot_setpoint.get() else { break 'oneshot };

            // The primary probe must be available
            let Some(primary_probe) = self.probes.get("primary").await else { break 'oneshot };

            // Check if the setpoint is completed
            match (
                setpoint.comparison,
                primary_probe.value().partial_cmp(&setpoint.setpoint),
            ) {
                (OneshotOrdering::Less, Some(cmp::Ordering::Less)) => {
                    self.oneshot_setpoint.set(None);
                    break 'oneshot;
                }
                (OneshotOrdering::Greater, Some(cmp::Ordering::Greater)) => {
                    self.oneshot_setpoint.set(None);
                    break 'oneshot;
                }
                _ => (),
            }

            // We are not complete, execute action
            return Some(setpoint.action);
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

impl models::mixer::Mixer for MixerState {
    fn mode(&self) -> HvacRequest {
        self.mode()
    }

    async fn get_probe_temp(&self, probe: &str) -> Option<f32> {
        self.probes.get(probe).await.map(|probe| probe.value())
    }
}

#[derive(Clone)]
pub struct Mixer {
    state: Arc<ArcCell<MixerState>>,
}

impl Mixer {
    pub fn new(state: Arc<MixerState>) -> Self {
        Mixer {
            state: Arc::new(ArcCell::new(state)),
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
        let new_rules = Arc::new(timed_rule::load(&self.state().redis).await);
        self.update_state(|state| state.timed_ruleset = new_rules);
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
