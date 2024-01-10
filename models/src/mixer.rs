use crate::hvac_request::HvacRequest;

pub trait Mixer {
    fn mode(&self) -> HvacRequest;
    fn get_probe_temp(&self, probe: &str) -> impl std::future::Future<Output = Option<f32>> + Send;
}
