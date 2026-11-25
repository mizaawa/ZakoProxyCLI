#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::path::Path::new(".env").exists() {
        dotenvy::dotenv()?;
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "zakoai_proxy=info".into()),
        )
        .init();
    let data = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into());
    let web = std::env::var("WEB_DIR").unwrap_or_else(|_| "web/dist".into());
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:1990".into());
    let state = zakoai_proxy::new_app(std::path::Path::new(&data), web.into())?;
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(address=%bind,"ZakoAI is listening");
    let balances = tokio::spawn(zakoai_proxy::providers::run_balance_refresh(state.clone()));
    let served = axum::serve(listener, zakoai_proxy::app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    balances.abort();
    let _ = balances.await;
    served?;
    Ok(())
}
