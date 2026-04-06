use chrono::Local;
use log::{debug, info};

mod config;
use config::{Config, init_config};

fn setup_logger() {
    fern::Dispatch::new()
        .format(|out, message, record| {
            if cfg!(debug_assertions) {
                let file = record.file().unwrap_or("unknown");
                let line = record
                    .line()
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "?".to_string());

                out.finish(format_args!(
                    "[{} {} {}:{}]: {}",
                    Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                    record.level(),
                    file,
                    line,
                    message
                ));
            } else {
                out.finish(format_args!(
                    "[{} {}]: {}",
                    Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                    record.level(),
                    message
                ));
            }
        })
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .apply()
        .unwrap();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    setup_logger();

    info!("Application Starting - v{}", env!("CARGO_PKG_VERSION"));

    // Load and verify config
    let config = Config::from_file("config.toml")?;
    let config = match config.verify() {
        Ok(verified_config) => {
            info!("Config verified successfully");
            verified_config
        }
        Err(e) => {
            log::error!("Config verification failed: {}", e);
            anyhow::bail!(e);
        }
    };

    // Initialize global config
    init_config(config);
    debug!("Loaded config: {:?}", config::get_config());

    Ok(())
}
