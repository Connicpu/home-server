use crate::hvac_request::HvacRequest;

pub trait Mixer {
    fn mode(&self) -> HvacRequest;
    async fn get_probe_temp(&self, probe: &str) -> Option<f32>;
}
