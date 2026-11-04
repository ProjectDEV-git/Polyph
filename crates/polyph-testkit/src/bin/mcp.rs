//! `polyph mcp` for tests: the daemon under test runs inside the test binary,
//! so agents need a separate executable for the MCP server.

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(e) = polyph_mcp::run_stdio().await {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
