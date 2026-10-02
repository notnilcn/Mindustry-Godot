// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server console TCP socket (plan 22 §3.6/§6.6).
//!
//! Port of `ServerControl`'s socket reader: a line-delimited UTF-8 loopback
//! connection, one active client at a time, each received line handled like
//! stdin and every formatted log line echoed back with colors stripped.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

/// A line-based command socket (loopback).
#[derive(Debug)]
pub struct CommandSocket {
    listener: Option<TcpListener>,
    reader: Option<BufReader<TcpStream>>,
    stream: Option<TcpStream>,
}

impl CommandSocket {
    /// A socket that never binds or reads (when `config socketInput` is off).
    pub fn disabled() -> Self {
        Self {
            listener: None,
            reader: None,
            stream: None,
        }
    }

    /// Binds `addr` (`socketInputAddress:socketInputPort`) and listens.
    pub fn bind(addr: &str) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener: Some(listener),
            reader: None,
            stream: None,
        })
    }

    /// Whether a listener is bound.
    pub fn is_enabled(&self) -> bool {
        self.listener.is_some()
    }

    /// The bound local address, if any.
    pub fn local_addr(&self) -> Option<std::net::SocketAddr> {
        self.listener.as_ref().and_then(|l| l.local_addr().ok())
    }

    /// Accepts a pending client (replacing the previous one), if any.
    pub fn accept(&mut self) -> std::io::Result<bool> {
        let Some(listener) = &self.listener else {
            return Ok(false);
        };
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                self.reader = Some(BufReader::new(stream.try_clone()?));
                self.stream = Some(stream);
                log::info!("[socket] client connected");
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Reads one line from the active client, if a full line is available.
    pub fn read_line(&mut self) -> std::io::Result<Option<String>> {
        let Some(reader) = &mut self.reader else {
            return Ok(None);
        };
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                self.reader = None;
                self.stream = None;
                Ok(None)
            }
            Ok(_) => Ok(Some(line.trim_end_matches(['\r', '\n']).to_owned())),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Echoes a formatted log line (colors stripped, one per `\n`).
    pub fn send_log(&mut self, line: &str) -> std::io::Result<()> {
        let Some(stream) = &mut self.stream else {
            return Ok(());
        };
        let cleaned = super::logs::strip_colors(line);
        stream.write_all(cleaned.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()
    }

    /// Writes a command response line (colors stripped, one per `\n`).
    pub fn send_line(&mut self, line: &str) -> std::io::Result<()> {
        self.send_log(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_roundtrip() {
        let mut server = CommandSocket::bind("127.0.0.1:0").expect("bind");
        let addr = server.local_addr().expect("addr");
        let mut client = TcpStream::connect(addr).expect("connect");
        server.accept().expect("accept");
        // The non-blocking listener accepted a blocking stream; reading may
        // need the client write to land first.
        client.write_all(b"status\n").expect("write");
        client.flush().expect("flush");
        let mut line = None;
        for _ in 0..100 {
            if let Some(read) = server.read_line().expect("read") {
                line = Some(read);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(line.as_deref(), Some("status"));
        server.send_log("[green]ok").expect("send");
        let _ = client.shutdown(std::net::Shutdown::Both);
    }

    #[test]
    fn disabled_is_noop() {
        let mut socket = CommandSocket::disabled();
        assert!(!socket.is_enabled());
        assert_eq!(socket.read_line().expect("read"), None);
        socket.send_line("hello").expect("send");
        assert!(!socket.accept().expect("accept"));
    }
}
