//! Output sink implementations
//!
//! Sinks forward telemetry data to various destinations (HTTP, UDP, file)

#![allow(dead_code)]

use crate::state::{SinkConfig, SinkType};
use anyhow::Result;
use ost_core::model::{FieldMask, TelemetryFrame};

/// Trait for output sinks
pub trait Sink: Send {
    fn send(&mut self, frame: &TelemetryFrame, mask: Option<&FieldMask>) -> Result<()>;
}

/// HTTP POST sink
pub struct HttpSink {
    url: String,
    client: reqwest::Client,
}

impl HttpSink {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: reqwest::Client::new(),
        }
    }
}

impl Sink for HttpSink {
    fn send(&mut self, frame: &TelemetryFrame, mask: Option<&FieldMask>) -> Result<()> {
        let json = frame.to_json_filtered(mask)?;
        // Fire and forget (non-blocking)
        let url = self.url.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            if let Err(e) = client.post(&url).body(json).send().await {
                tracing::warn!("HTTP sink error: {}", e);
            }
        });
        Ok(())
    }
}

/// UDP sink
pub struct UdpSink {
    socket: std::net::UdpSocket,
    addr: std::net::SocketAddr,
}

impl UdpSink {
    pub fn new(host: String, port: u16) -> Result<Self> {
        let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;
        let addr = format!("{}:{}", host, port).parse()?;
        Ok(Self { socket, addr })
    }
}

impl Sink for UdpSink {
    fn send(&mut self, frame: &TelemetryFrame, mask: Option<&FieldMask>) -> Result<()> {
        let json = frame.to_json_filtered(mask)?;
        self.socket.send_to(json.as_bytes(), self.addr)?;
        Ok(())
    }
}

/// File sink (NDJSON)
pub struct FileSink {
    file: std::fs::File,
}

impl FileSink {
    pub fn new(path: String) -> Result<Self> {
        use std::fs::OpenOptions;
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { file })
    }
}

impl Sink for FileSink {
    fn send(&mut self, frame: &TelemetryFrame, mask: Option<&FieldMask>) -> Result<()> {
        use std::io::Write;
        let json = frame.to_json_filtered(mask)?;
        writeln!(self.file, "{}", json)?;
        Ok(())
    }
}

/// Create a sink from configuration
pub fn create_sink(config: &SinkConfig) -> Result<Box<dyn Sink>> {
    match &config.sink_type {
        SinkType::Http { url } => Ok(Box::new(HttpSink::new(url.clone()))),
        SinkType::Udp { host, port } => Ok(Box::new(UdpSink::new(host.clone(), *port)?)),
        SinkType::File { path } => Ok(Box::new(FileSink::new(path.clone())?)),
    }
}
