//! Socket I/O only: ureq owns HTTP framing, parsing, and chunk decoding.
//!
//! A one-shot connector cannot fall back to TCP, a proxy, or a redirected destination. Keep
//! ureq's unversioned extension API confined here and test it whenever the pinned version changes.

use std::io::{self, Read, Write};
use std::net::TcpStream;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ureq::unversioned::resolver::{ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, LazyBuffers, NextTimeout, Transport,
};

const CANCEL_POLL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub(crate) enum Socket {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(UnixStream),
}

impl Socket {
    fn read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.set_read_timeout(timeout),
            #[cfg(unix)]
            Self::Unix(stream) => stream.set_read_timeout(timeout),
        }
    }

    fn write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.set_write_timeout(timeout),
            #[cfg(unix)]
            Self::Unix(stream) => stream.set_write_timeout(timeout),
        }
    }
}

impl Read for Socket {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.read(bytes),
            #[cfg(unix)]
            Self::Unix(stream) => stream.read(bytes),
        }
    }
}

impl Write for Socket {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(stream) => stream.write(bytes),
            #[cfg(unix)]
            Self::Unix(stream) => stream.write(bytes),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.flush(),
            #[cfg(unix)]
            Self::Unix(stream) => stream.flush(),
        }
    }
}

/// No DNS lookup is needed: the caller has already opened the validated local socket.
#[derive(Debug)]
struct ConnectedResolver;

impl Resolver for ConnectedResolver {
    fn resolve(
        &self,
        _uri: &ureq::http::Uri,
        _config: &ureq::config::Config,
        _timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let mut addresses = self.empty();
        addresses.push(([127, 0, 0, 1], 0).into());
        Ok(addresses)
    }
}

#[derive(Debug)]
struct ConnectedSocket(Mutex<Option<SocketTransport>>);

impl Connector for ConnectedSocket {
    type Out = SocketTransport;

    fn connect(
        &self,
        _details: &ConnectionDetails,
        _chained: Option<()>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        self.0
            .lock()
            .map_err(|_| ureq::Error::ConnectionFailed)?
            .take()
            .map(Some)
            .ok_or(ureq::Error::ConnectionFailed)
    }
}

struct SocketTransport {
    socket: Socket,
    buffers: LazyBuffers,
    cancelled: Option<Arc<AtomicBool>>,
}

impl std::fmt::Debug for SocketTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Wire buffers can contain controller credentials; never expose them through Debug.
        formatter
            .debug_struct("SocketTransport")
            .field("socket", &self.socket)
            .field("cancellable", &self.cancelled.is_some())
            .finish_non_exhaustive()
    }
}

impl Transport for SocketTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.socket.write_timeout(timeout.not_zero().map(|t| *t))?;
        self.socket.write_all(&self.buffers.output()[..amount])?;
        Ok(())
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let started = Instant::now();
        let budget = timeout.not_zero().map(|t| *t);
        loop {
            if self
                .cancelled
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Relaxed))
            {
                // Not Interrupted: Read::read_to_end may automatically retry Interrupted.
                return Err(io::Error::from(io::ErrorKind::ConnectionAborted).into());
            }
            let remaining = match budget {
                Some(budget) => Some(
                    budget
                        .checked_sub(started.elapsed())
                        .filter(|remaining| !remaining.is_zero())
                        .ok_or(ureq::Error::Timeout(timeout.reason))?,
                ),
                None => None,
            };
            let poll = if self.cancelled.is_some() {
                Some(remaining.map_or(CANCEL_POLL, |left| left.min(CANCEL_POLL)))
            } else {
                remaining
            };
            self.socket.read_timeout(poll)?;
            match self.socket.read(self.buffers.input_append_buf()) {
                Ok(amount) => {
                    self.buffers.input_appended(amount);
                    return Ok(amount > 0);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                    ) =>
                {
                    // Remain inside this I/O call: ureq retains partially decoded headers/chunks.
                    if self.cancelled.is_none() {
                        return Err(ureq::Error::Timeout(timeout.reason));
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
    }

    fn is_open(&mut self) -> bool {
        false // Never pool/reuse a privileged controller connection.
    }
}

pub(crate) fn agent(
    socket: Socket,
    config: ureq::config::Config,
    cancelled: Option<Arc<AtomicBool>>,
) -> ureq::Agent {
    let transport = SocketTransport {
        socket,
        buffers: LazyBuffers::new(config.input_buffer_size(), config.output_buffer_size()),
        cancelled,
    };
    ureq::Agent::with_parts(
        config,
        ConnectedSocket(Mutex::new(Some(transport))),
        ConnectedResolver,
    )
}
