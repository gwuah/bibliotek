use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::{Context, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::build;
use crate::config::Config;

const TIMEOUT: Duration = Duration::from_secs(3);

pub async fn check(config: &Config) -> anyhow::Result<String> {
    let address = reachable_address(config.bind);
    let body = tokio::time::timeout(TIMEOUT, fetch_health(address))
        .await
        .with_context(|| format!("no answer from {address} within {TIMEOUT:?}"))??;

    let health: serde_json::Value = serde_json::from_str(&body).context("health response is not JSON")?;
    let running = health["data"]["revision"].as_str().unwrap_or("unknown");
    if running != build::GIT_REVISION {
        bail!(
            "{address} is running revision {running}, not this binary's {}",
            build::GIT_REVISION
        );
    }
    Ok(format!("{address} is healthy at revision {running}"))
}

fn reachable_address(bind: SocketAddr) -> SocketAddr {
    if bind.ip().is_unspecified() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), bind.port())
    } else {
        bind
    }
}

async fn fetch_health(address: SocketAddr) -> anyhow::Result<String> {
    let mut stream = TcpStream::connect(address)
        .await
        .with_context(|| format!("nothing is listening on {address}"))?;
    let request = format!("GET /api/health HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;

    let mut response = String::new();
    stream.read_to_string(&mut response).await?;

    let (head, body) = response
        .split_once("\r\n\r\n")
        .context("malformed HTTP response")?;
    if !head.starts_with("HTTP/1.1 200") {
        bail!(
            "health check answered `{}`",
            head.lines().next().unwrap_or_default()
        );
    }
    Ok(body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_binds_are_checked_through_loopback() {
        assert_eq!(
            reachable_address("0.0.0.0:6578".parse().unwrap()),
            "127.0.0.1:6578".parse().unwrap()
        );
        assert_eq!(
            reachable_address("192.168.1.9:6578".parse().unwrap()),
            "192.168.1.9:6578".parse().unwrap()
        );
    }
}
