use std::sync::{
    atomic::{AtomicI64, AtomicU32, Ordering},
    Arc,
};

#[derive(Clone)]
pub struct Probe {
    inner: Arc<ProbeInner>,
}

impl Probe {
    pub fn new(name: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Probe {
            inner: Arc::new(ProbeInner {
                name: name.into(),
                endpoint: endpoint.into(),
                value: AtomicU32::new(f32::to_bits(f32::NAN)),
                last_update: AtomicI64::new(current_timestamp()),
            }),
        }
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }

    pub fn endpoint(&self) -> &str {
        &self.inner.endpoint
    }

    pub fn value(&self) -> f32 {
        f32::from_bits(self.inner.value.load(Ordering::SeqCst))
    }

    pub fn update(&self, value: f32) {
        self.inner
            .value
            .store(f32::to_bits(value), Ordering::SeqCst);
        self.inner
            .last_update
            .store(current_timestamp(), Ordering::SeqCst);
    }

    pub fn last_update(&self) -> i64 {
        self.inner.last_update.load(Ordering::SeqCst)
    }
}

fn current_timestamp() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

struct ProbeInner {
    name: String,
    endpoint: String,
    value: AtomicU32,
    last_update: AtomicI64,
}
