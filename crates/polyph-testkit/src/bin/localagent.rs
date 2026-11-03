//! The local agent, built inside the test kit so tests can find it.

#[tokio::main]
async fn main() {
    polyph_localagent::run().await;
}
