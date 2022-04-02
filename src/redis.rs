use redis::{aio::ConnectionManager, Client};

#[derive(Clone)]
pub struct RedisConn {
    connection: ConnectionManager,
}

impl RedisConn {
    pub async fn open(host: impl Into<String>, port: u16) -> anyhow::Result<Self> {
        let client = Client::open((host, port))?;
        Ok(RedisConn {
            connection: ConnectionManager::new(client).await?,
        })
    }

    pub fn get(&self) -> ConnectionManager {
        self.connection.clone()
    }
}
