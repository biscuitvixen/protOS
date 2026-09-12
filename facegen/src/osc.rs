//! OSC over UDP: the bus every producer speaks.
//!
//! One blocking receiver thread owns the socket. Every message is an
//! address plus one number; the address resolves through the contract
//! and the value lands in the shared input store. Bundles are walked
//! recursively. Unknown addresses are logged once each so a chatty
//! producer with a prefix misconfigured shows up in the log without
//! flooding it. The producer side is a small helper the fake source
//! and tests use to send the same wire format.

use std::collections::HashSet;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context;
use rosc::{OscMessage, OscPacket, OscType};

use crate::contract::InputStore;

/// Babble's default output port; facegen is the single listener.
pub const DEFAULT_PORT: u16 = 8888;

/// Largest datagram accepted; OSC over UDP stays well under this.
const MAX_DATAGRAM: usize = 4096;

/// How often the receiver logs its message rate.
const STATS_PERIOD: Duration = Duration::from_secs(5);

/// Bind the socket and start the receiver thread.
pub fn start_receiver(
    bind: SocketAddr,
    inputs: Arc<Mutex<InputStore>>,
) -> anyhow::Result<thread::JoinHandle<()>> {
    let socket =
        UdpSocket::bind(bind).with_context(|| format!("cannot bind OSC socket on {bind}"))?;
    socket
        .set_read_timeout(Some(STATS_PERIOD))
        .context("setting the OSC socket timeout")?;
    tracing::info!(%bind, "osc listening");
    Ok(thread::Builder::new()
        .name("osc".into())
        .spawn(move || receive_loop(socket, inputs))?)
}

fn receive_loop(socket: UdpSocket, inputs: Arc<Mutex<InputStore>>) {
    let mut buf = [0u8; MAX_DATAGRAM];
    let mut warned = HashSet::new();
    let mut stats = Stats::new();
    loop {
        match socket.recv_from(&mut buf) {
            Ok((len, _peer)) => match rosc::decoder::decode_udp(&buf[..len]) {
                Ok((_, packet)) => {
                    let now = Instant::now();
                    let mut store = inputs.lock().expect("input store lock");
                    apply_packet(&packet, &mut store, now, &mut warned, &mut stats);
                }
                Err(e) => {
                    stats.malformed += 1;
                    tracing::debug!("malformed OSC datagram: {e}");
                }
            },
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => {
                tracing::error!("osc receive failed: {e}");
                return;
            }
        }
        stats.maybe_log();
    }
}

/// Route one packet's messages into the store.
pub fn apply_packet(
    packet: &OscPacket,
    store: &mut InputStore,
    now: Instant,
    warned: &mut HashSet<String>,
    stats: &mut Stats,
) {
    match packet {
        OscPacket::Bundle(bundle) => {
            for inner in &bundle.content {
                apply_packet(inner, store, now, warned, stats);
            }
        }
        OscPacket::Message(message) => {
            stats.messages += 1;
            let Some(value) = first_number(message) else {
                stats.ignored += 1;
                return;
            };
            if !store.set_by_address(&message.addr, value, now) {
                stats.ignored += 1;
                if warned.insert(message.addr.clone()) {
                    tracing::warn!(address = %message.addr, "ignoring OSC address not in the contract");
                }
            }
        }
    }
}

/// The message's first argument as a float; Babble sends one float,
/// other tools sometimes send ints or doubles.
fn first_number(message: &OscMessage) -> Option<f32> {
    match message.args.first()? {
        OscType::Float(v) => Some(*v),
        OscType::Double(v) => Some(*v as f32),
        OscType::Int(v) => Some(*v as f32),
        OscType::Long(v) => Some(*v as f32),
        OscType::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// Message counters, logged every `STATS_PERIOD`.
pub struct Stats {
    since: Instant,
    pub messages: u64,
    pub ignored: u64,
    pub malformed: u64,
}

impl Default for Stats {
    fn default() -> Self {
        Self::new()
    }
}

impl Stats {
    pub fn new() -> Self {
        Self {
            since: Instant::now(),
            messages: 0,
            ignored: 0,
            malformed: 0,
        }
    }

    fn maybe_log(&mut self) {
        let elapsed = self.since.elapsed();
        if elapsed < STATS_PERIOD {
            return;
        }
        if self.messages > 0 || self.malformed > 0 {
            tracing::info!(
                per_second = format_args!("{:.0}", self.messages as f64 / elapsed.as_secs_f64()),
                ignored = self.ignored,
                malformed = self.malformed,
                "osc"
            );
        }
        *self = Self::new();
    }
}

/// A producer: one socket, one destination, one float per message.
pub struct Sender {
    socket: UdpSocket,
    to: SocketAddr,
}

impl Sender {
    pub fn new(to: impl ToSocketAddrs) -> anyhow::Result<Self> {
        let to = to
            .to_socket_addrs()?
            .next()
            .context("OSC destination did not resolve")?;
        let bind: SocketAddr = if to.is_ipv4() {
            "0.0.0.0:0".parse()?
        } else {
            "[::]:0".parse()?
        };
        Ok(Self {
            socket: UdpSocket::bind(bind)?,
            to,
        })
    }

    pub fn send(&self, address: &str, value: f32) -> anyhow::Result<()> {
        let packet = OscPacket::Message(OscMessage {
            addr: address.to_string(),
            args: vec![OscType::Float(value)],
        });
        let bytes = rosc::encoder::encode(&packet)?;
        self.socket.send_to(&bytes, self.to)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::lookup_name;
    use rosc::OscBundle;

    #[test]
    fn messages_and_bundles_land_in_the_store_and_unknown_addresses_are_warned_once() {
        let mut store = InputStore::new();
        let mut warned = HashSet::new();
        let mut stats = Stats::new();
        let now = Instant::now();
        let msg = |addr: &str, arg: OscType| {
            OscPacket::Message(OscMessage {
                addr: addr.into(),
                args: vec![arg],
            })
        };
        let bundle = OscPacket::Bundle(OscBundle {
            timetag: rosc::OscTime::from((0, 0)),
            content: vec![
                msg("/jawOpen", OscType::Float(0.7)),
                msg("/protos/eye/left/x", OscType::Double(-0.25)),
                msg("/mouthSmileLeft", OscType::Int(1)),
                msg("/avatar/parameters/jawOpen", OscType::Float(1.0)),
                msg("/avatar/parameters/jawOpen", OscType::Float(1.0)),
                msg("/jawOpen", OscType::String("no".into())),
            ],
        });
        apply_packet(&bundle, &mut store, now, &mut warned, &mut stats);
        assert!(
            (store.get(lookup_name("jawOpen").unwrap()) - 0.7).abs() < 1e-6,
            "float message applied"
        );
        assert!(
            (store.get(lookup_name("eyeLeftX").unwrap()) + 0.25).abs() < 1e-6,
            "double message applied"
        );
        assert_eq!(
            store.get(lookup_name("mouthSmileLeft").unwrap()),
            1.0,
            "int message applied"
        );
        assert_eq!(stats.messages, 6, "every message counted");
        assert_eq!(
            stats.ignored, 3,
            "two unknown addresses and one non-numeric argument ignored"
        );
        assert_eq!(warned.len(), 1, "an unknown address is remembered once");
    }

    #[test]
    fn the_receiver_thread_applies_datagrams_sent_over_loopback() {
        let inputs = Arc::new(Mutex::new(InputStore::new()));
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let bind = socket.local_addr().unwrap();
        drop(socket);
        let _thread = start_receiver(bind, Arc::clone(&inputs)).unwrap();
        let sender = Sender::new(bind).unwrap();
        let jaw = lookup_name("jawOpen").unwrap();
        for _ in 0..50 {
            sender.send("/jawOpen", 0.42).unwrap();
            thread::sleep(Duration::from_millis(10));
            if (inputs.lock().unwrap().get(jaw) - 0.42).abs() < 1e-6 {
                return;
            }
        }
        panic!("the receiver never applied the datagram");
    }
}
