use std::{collections::HashMap, str::FromStr, sync::Arc, time::Duration};

use redis::AsyncCommands;
use tokio::sync::RwLock;

use crate::{mqtt::MqttClient, RedisConn, api::atticfan::FanState};

use self::{
    mixer::{AtomicHvacRequest, HvacRequest, Mixer, MixerState},
    probe::Probe,
};

pub mod mixer;
pub mod probe;

#[derive(Clone)]
pub struct HvacState {
    pub probes: Probes,
    pub mixer: Mixer,
    pub hvac_mode: Arc<AtomicHvacRequest>
}

pub const PROBE_ENDPOINTS: &str = "thermostat.config.probe_endpoints";
pub const CONFIG_MODE: &str = "thermostat.config.mode";
pub const PROBE_HISTORY: &str = "thermostat.probes.history";
pub const PINSTATE_HISTORY: &str = "thermostat.pinstate.history";

pub async fn initialize(mqtt: &MqttClient, redis: &RedisConn, fan_state: &FanState) -> anyhow::Result<HvacState> {
    // Create the primary probe
    let probes: Probes = Default::default();
    init_probe(&probes, mqtt, Probe::new("primary", "home/thermostat/temp")).await;

    // Get additional configured probes
    let probe_endpoints: HashMap<String, String> = {
        let mut redis = redis.get();
        redis.hgetall(PROBE_ENDPOINTS).await.unwrap_or_default()
    };
    for (name, endpoint) in probe_endpoints {
        init_probe(&probes, mqtt, Probe::new(name, endpoint)).await;
    }

    // Create a handler for the HVAC Mode
    let hvac_mode = Arc::new(AtomicHvacRequest::new());

    // Try to get the last known hvac mode first
    hvac_mode.store(
        {
            let mut redis = redis.get();
            redis.get(CONFIG_MODE).await
        }
        .ok()
        .and_then(|mode: String| HvacRequest::from_payload(mode.as_bytes()))
        .unwrap_or(HvacRequest::Heat),
    );

    // Set up a handler to request it from the thermostat unit
    mqtt.subscribe("home/thermostat/hvac/mode").await;
    {
        let hvac_mode = hvac_mode.clone();
        mqtt.handle("home/thermostat/hvac/mode", move |_, payload| {
            if let Some(mode) = HvacRequest::from_payload(payload) {
                hvac_mode.store(mode);
            }
        })
        .await;
    }

    // We should request the mode periodically
    {
        let mqtt = mqtt.clone();
        crate::spawn("hvac_mode_checker", async move {
            loop {
                mqtt.publish("home/thermostat/hvac/mode/get", b"").await;
                tokio::time::sleep(Duration::from_secs(500)).await;
            }
        });
    }

    // Create the mixer
    let mixer_state = MixerState::new(redis, probes.clone(), hvac_mode.clone(), fan_state.clone()).await;
    let mixer = Mixer::new(mixer_state);

    // Create the mix sender
    {
        let mqtt = mqtt.clone();
        let mixer = mixer.clone();
        crate::spawn("hvac_state_setter", async move {
            loop {
                let request = mixer.query().await;
                mqtt.publish("home/thermostat/hvac/remotestate/set", request.payload())
                    .await;
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });
    }

    // Create the probe historian
    {
        let redis = redis.clone();
        let probes = probes.clone();
        crate::spawn("probe_historian", async move {
            const PERIOD: u32 = 10;
            const MAX_LEN: u32 = 60 * 60 * 24 * 366 / PERIOD; // Store ~1 year
            let probe_push = redis::Script::new(&format!(
                r#"
                for i = 1, #KEYS do
                    local history_key = KEYS[i]
                    local last_value = redis.call('LINDEX', history_key, 0)
                    if last_value ~= ARGV[i] then
                        redis.call('LPUSH', history_key, ARGV[i])
                    end
                    redis.call('LTRIM', 0, {MAX_LEN}-1)
                end
                return #KEYS
            "#
            ));

            loop {
                tokio::time::sleep(Duration::from_secs(PERIOD as u64 / 2)).await;

                // Store all of the probe's latest values into redis with a timestamp
                let probes = probes.probes.read().await;

                let mut invocation = probe_push.prepare_invoke();
                for probe in probes.values() {
                    let value = probe.value();
                    if value.is_nan() {
                        continue;
                    }
                    invocation.key(
                        format!("{PROBE_HISTORY}:{}", probe.name())
                    ).arg(format!(
                        "{time}:{value}",
                        value = probe.value(),
                        time = probe.last_update()
                    ));
                }

                let mut redis = redis.get();
                let _: Result<(), _> = invocation.invoke_async(&mut redis).await;
            }
        });
    }

    // Create the state historian
    {
        let redis = redis.clone();
        let mqtt = mqtt.clone();

        let record_pinstate = redis::Script::new(&format!(
            r#"
            local state = ARGV[1]
            local time = ARGV[2]
            local latest = redis.call('LINDEX', KEYS[1], 0)
            if latest == nil or string.sub(latest, 1, #state) ~= state then
                return redis.call('LPUSH', KEYS[1], state .. ':' .. time)
            else
                return 0
            end
        "#
        ));

        mqtt.subscribe("home/thermostat/hvac/pinstate").await;
        mqtt.handle("home/thermostat/hvac/pinstate", move |_, payload| {
            let now = chrono::Utc::now().timestamp_millis();
            if let Some(state) = HvacRequest::from_payload(payload) {
                let record_pinstate = record_pinstate.clone();
                let redis = redis.clone();
                crate::spawn("record_pinstate", async move {
                    let mut redis = redis.get();
                    record_pinstate
                        .prepare_invoke()
                        .key(PINSTATE_HISTORY)
                        .arg(state.payload_str())
                        .arg(now)
                        .invoke_async::<_, ()>(&mut redis)
                        .await
                        .ok(); // Ignore failures it's nbd
                });
            }
        })
        .await;
    }

    // Periodically query the pinstate so we can record it even in the advent of hiccups
    {
        let mqtt = mqtt.clone();
        crate::spawn("pinstate_query", async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                mqtt.publish("home/thermostat/hvac/pinstate/get", b"").await;
            }
        })
    }

    // Create the final HVAC state
    Ok(HvacState { probes, mixer, hvac_mode })
}

#[derive(Default, Clone)]
pub struct Probes {
    probes: Arc<RwLock<HashMap<String, Probe>>>,
}

impl Probes {
    pub async fn get(&self, name: &str) -> Option<Probe> {
        self.probes.read().await.get(name).cloned()
    }

    pub async fn create_probe(
        &self,
        redis: &RedisConn,
        mqtt: &MqttClient,
        name: &str,
        endpoint: &str,
    ) -> anyhow::Result<()> {
        let mut redis = redis.get();
        let () = redis.hset(PROBE_ENDPOINTS, name, endpoint).await?;
        init_probe(self, mqtt, Probe::new(name, endpoint)).await;
        Ok(())
    }

    pub async fn delete_probe(&self, redis: &RedisConn, name: &str) -> anyhow::Result<()> {
        self.probes.write().await.remove(name);
        let mut redis = redis.get();
        let () = redis.hdel(PROBE_ENDPOINTS, name).await?;
        Ok(())
    }

    pub async fn keys(&self) -> Vec<String> {
        self.probes.read().await.keys().cloned().collect()
    }
}

async fn init_probe(probes: &Probes, mqtt: &MqttClient, probe: Probe) {
    probes
        .probes
        .write()
        .await
        .insert(probe.name().to_string(), probe.clone());
    let endpoint = probe.endpoint().to_owned();
    mqtt.subscribe(&endpoint).await;
    mqtt.handle(&endpoint, move |_topic, payload| {
        if let Some(temp) = std::str::from_utf8(payload)
            .ok()
            .and_then(|s| f32::from_str(s).ok())
        {
            probe.update(temp);
        }
    })
    .await;
}
