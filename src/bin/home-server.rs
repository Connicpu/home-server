#[tokio::main]
async fn main() -> anyhow::Result<()> {
    home_server::run_server().await
}
