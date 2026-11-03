//! `polyph-localagent`: an ACP agent for OpenAI-compatible endpoints
//! (Ollama, llama.cpp server, LM Studio, vLLM). Configured with
//! `POLYPH_LOCAL_ENDPOINT`, `POLYPH_LOCAL_MODEL`, `POLYPH_LOCAL_API_KEY`,
//! `POLYPH_LOCAL_CONTEXT` (tokens) and `POLYPH_LOCAL_TOOLS` (`native` or `text`).

#[tokio::main]
async fn main() {
    polyph_localagent::run().await;
}
