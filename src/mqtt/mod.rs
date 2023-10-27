use std::sync::Arc;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use tokio::sync::RwLock;
use tracing::Instrument;

use self::handler::Router;

pub mod handler;

#[derive(Clone)]
pub struct MqttClient {
    client: AsyncClient,
    router: Arc<RwLock<Router>>,
}

impl MqttClient {
    pub async fn subscribe(&self, topic: &str) {
        self.client.subscribe(topic, QoS::AtMostOnce).await.unwrap();
    }

    pub async fn handle(&self, path: &str, handler: impl Fn(&str, &[u8]) + Send + Sync + 'static) {
        let mut router = self.router.write().await;
        router.insert(path, handler.into());
    }

    pub async fn publish(&self, topic: &str, payload: &[u8]) {
        self.client
            .publish(topic, QoS::AtMostOnce, false, payload)
            .await
            .unwrap();
    }
}

#[tracing::instrument]
pub fn init(options: MqttOptions) -> MqttClient {
    let (client, eventloop) = AsyncClient::new(options, 50);
    let router = Arc::new(RwLock::new(Router::new()));

    crate::spawn(
        "mqtt_listener",
        mqtt_listener(eventloop, router.clone()).instrument(tracing::info_span!("mqtt_listener")),
    );

    MqttClient { client, router }
}

async fn mqtt_listener(mut eventloop: rumqttc::EventLoop, router: Arc<RwLock<Router>>) {
    loop {
        let notification = eventloop.poll().await.expect(":glares:");

        if let Event::Incoming(Packet::Publish(packet)) = notification {
            let router = router.clone();
            tokio::task::spawn_blocking(move || {
                let router = router.blocking_write();
                router.dispatch(&packet.topic, &packet.payload);
            }).await.expect("Nothing should go wrong");
        }
    }
}
