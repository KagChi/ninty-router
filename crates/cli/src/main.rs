use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;

#[derive(Parser)]
#[command(name = "ninty-router", about = "Local AI router with dashboard")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = ninty_core::config::DEFAULT_PORT)]
    port: u16,

    /// Host/interface to bind
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,

    /// Do not open the dashboard in a browser
    #[arg(long)]
    no_browser: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();

    let db_path = ninty_core::config::db_path();
    let db = match server::db::Db::open(&db_path) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("failed to open database {}: {e}", db_path.display());
            return ExitCode::FAILURE;
        }
    };

    let state = Arc::new(server::state::AppState::new(db));

    println!("ninty-router");
    println!("  dashboard:  http://{}:{}/", args.host, args.port);
    println!("  openai api: http://{}:{}/v1", args.host, args.port);
    println!("  data dir:   {}", ninty_core::config::data_dir().display());

    if !args.no_browser {
        let url = format!("http://{}:{}/", args.host, args.port);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            open_browser(&url);
        });
    }

    match server::run(state, &args.host, args.port).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("server error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let cmd = ("open", url);
    #[cfg(target_os = "linux")]
    let cmd = ("xdg-open", url);
    #[cfg(target_os = "windows")]
    let cmd = ("cmd", "/c start");
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    return;

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    {
        let _ = std::process::Command::new(cmd.0).arg(cmd.1).spawn();
    }
}
