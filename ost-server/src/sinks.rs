//! Output sink implementations
//!
//! Sinks forward telemetry data to UDP destinations

#![allow(dead_code)]

use crate::state::SinkConfig;
use anyhow::Result;
use ost_core::model::{MetricMask, TelemetryFrame};

/// Trait for output sinks
pub trait Sink: Send {
    fn send(&mut self, frame: &TelemetryFrame, mask: Option<&MetricMask>) -> Result<()>;
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
    fn send(&mut self, frame: &TelemetryFrame, mask: Option<&MetricMask>) -> Result<()> {
        let json = frame.to_json_filtered(mask)?;
        self.socket.send_to(json.as_bytes(), self.addr)?;
        Ok(())
    }
}

/// Create a sink from configuration
pub fn create_sink(config: &SinkConfig) -> Result<Box<dyn Sink>> {
    Ok(Box::new(UdpSink::new(config.host.clone(), config.port)?))
}
