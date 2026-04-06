use std::net::SocketAddrV4;

use chrono::Local;
use log::{debug, info};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

mod config;
use config::{Config, Protocol, init_config};

use crate::config::get_config;

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

async fn handle_http(port: u16, routes: Vec<config::RouteMapping>) {
    let addr = SocketAddrV4::new([0, 0, 0, 0].into(), port);

    let listener = match TcpListener::bind(&addr).await {
        Ok(it) => it,
        Err(err) => {
            log::error!("Failed to bind HTTP listener on {}: {}", addr, err);
            panic!("Failed to bind HTTP listener on {}: {}", addr, err);
        }
    };
    info!("HTTP listener bound to {}", addr);
    debug!("Listener: {:?}", listener);
    debug!("Routes configured: {:?}", routes);
    loop {
        let (mut socket, addr) = match listener.accept().await {
            Ok(it) => it,
            Err(err) => {
                log::error!("Failed to accept connection on {}: {}", addr, err);
                continue;
            }
        };
        info!("Accepted connection from {}", addr);
        // Use routing info to route the connection

        // Check which route that matched with the match_rule
        let mut s = String::new();
        socket.read_to_string(&mut s).await.unwrap();
        debug!("Received data: {}", s);
        if let Some(route_mapping) = routes.first() {
            debug!("Routing to service: {:?}", route_mapping.service);
        }
        socket.shutdown().await.unwrap();
        info!("Shutdown connection from {}", addr);
    }
}

async fn handle_https(port: u16, routes: Vec<config::RouteMapping>) {
    let addr = SocketAddrV4::new([0, 0, 0, 0].into(), port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(it) => it,
        Err(err) => {
            log::error!("Failed to bind HTTPS listener on {}: {}", addr, err);
            panic!("Failed to bind HTTPS listener on {}: {}", addr, err);
        }
    };
    info!("HTTPS listener bound to {}", addr);
    debug!("Listener: {:?}", listener);
    debug!("Routes configured: {:?}", routes);
    loop {
        let (mut socket, addr) = match listener.accept().await {
            Ok(it) => it,
            Err(err) => {
                log::error!("Failed to accept connection on {}: {}", addr, err);
                continue;
            }
        };
        info!("Accepted connection from {}", addr);
        // Use routing info to route the connection
        if let Some(route_mapping) = routes.first() {
            debug!("Routing to service: {:?}", route_mapping.service);
        }
        socket.shutdown().await.unwrap();
        info!("Shutdown connection from {}", addr);
    }
}

async fn handle_tcp(port: u16, routes: Vec<config::RouteMapping>) {
    let addr = SocketAddrV4::new([0, 0, 0, 0].into(), port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(it) => it,
        Err(err) => {
            log::error!("Failed to bind TCP listener on {}: {}", addr, err);
            panic!("Failed to bind TCP listener on {}: {}", addr, err);
        }
    };
    info!("TCP listener bound to {}", addr);
    debug!("Listener: {:?}", listener);
    debug!("Routes configured: {:?}", routes);
    loop {
        let (mut socket, addr) = match listener.accept().await {
            Ok(it) => it,
            Err(err) => {
                log::error!("Failed to accept connection on {}: {}", addr, err);
                continue;
            }
        };
        info!("Accepted connection from {}", addr);
        // Use routing info to route the connection
        if let Some(route_mapping) = routes.first() {
            debug!("Routing to service: {:?}", route_mapping.service);
        }
        socket.shutdown().await.unwrap();
        info!("Shutdown connection from {}", addr);
    }
}

fn setup_entry_points() -> anyhow::Result<Vec<tokio::task::JoinHandle<()>>> {
    let routing_table = get_config().build_routing_table();
    let mut listeners = Vec::new();

    for (entry_point_name, routes) in routing_table {
        debug!(
            "Setting up entry point: {} with {} routes",
            entry_point_name,
            routes.len()
        );

        if let Some(first_route) = routes.first() {
            let port = first_route.entry_point.port();
            let routes_clone = routes.clone();

            match first_route.entry_point.protocol() {
                Protocol::Http => {
                    let task = tokio::spawn(handle_http(port, routes_clone));
                    listeners.push(task);
                }
                Protocol::Https => {
                    let task = tokio::spawn(handle_https(port, routes_clone));
                    listeners.push(task);
                }
                Protocol::Tcp => {
                    let task = tokio::spawn(handle_tcp(port, routes_clone));
                    listeners.push(task);
                }
            }
        }
    }
    Ok(listeners)
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

    debug!("Setting up entry points");
    let entry_points = setup_entry_points()?;
    debug!("Setting up entry points completed");
    for handle in entry_points {
        handle.await?;
    }

    Ok(())
}
