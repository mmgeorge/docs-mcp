use anyhow::Result;
use docs_mcp::{server::DocsMcpServer, tools::AppState};
use rmcp::ServiceExt;
use rmcp::transport::io::stdio;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging to stderr (stdout is used for MCP protocol)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("docs_mcp=info".parse()?),
        )
        .init();

    let state = AppState::new().await?;
    let server = DocsMcpServer::new_with_state(Arc::new(state));

    let running = server.serve(stdio()).await?;
    running.waiting().await?;

    Ok(())
}
