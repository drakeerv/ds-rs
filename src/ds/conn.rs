use super::Signal;

use crate::proto::udp::inbound::UdpResponsePacket;
use crate::proto::udp::outbound::types::tags::{DateTime as DTTag, *};

use chrono::{Datelike, Timelike, Utc};
use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::time::timeout;
use tokio_util::codec::Decoder;
use tokio_util::udp::UdpFramed;

use crate::Result;
use crate::proto::tcp::DsTcpCodec;
use crate::proto::udp::DsUdpCodec;

use crate::ds::state::{DsMode, DsState};
use crate::proto::tcp::outbound::TcpTag;

mod backoff;

use backoff::ExponentialBackoff;
use std::io::ErrorKind;

/// The root task of the tokio runtime.
///
/// This task connects to the receiving UDP port, and spawns tasks for UDP sending, and for TCP communications once the connection to the RIO has been established.
pub(crate) async fn udp_conn(
    state: Arc<DsState>,
    mut target_ip: String,
    mut rx: UnboundedReceiver<Signal>,
) -> Result<()> {
    let mut tcp_connected = false;
    let mut tcp_tx = None;

    let udp_rx = UdpSocket::bind("0.0.0.0:1150").await?;
    let mut udp_rx = UdpFramed::new(udp_rx, DsUdpCodec);

    let (fwd_tx, mut fwd_rx) = unbounded_channel::<Signal>();

    let send_state = state.clone();
    let target = target_ip.clone();
    tokio::spawn(async move {
        let mut udp_tx = UdpSocket::bind("0.0.0.0:0")
            .await
            .expect("Failed to bind tx socket");
        udp_tx
            .connect(&format!("{}:1110", target))
            .await
            .expect("Failed to connect to target");

        let mut interval = tokio::time::interval(Duration::from_millis(20));

        //let mut stream = select(interval, fwd_rx);
        let mut backoff = ExponentialBackoff::new(Duration::new(5, 0));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let mut state = send_state.send().write().await;
                    let v = state.control().encode();
                    // Massively overengineered considering the _only_ time that this actually starts
                    // to come into play is directly after the simulator is closed before the DS switches to Normal mode again
                    // but I don't feel like changing it, and now it's fail safe
                    match backoff.run(udp_tx.send(&v[..])).await {
                        Ok(_) => {}
                        Err((e, dc)) => {
                            if e.kind() == ErrorKind::ConnectionRefused && dc {
                                println!("Send socket disconnected");
                                send_state.recv().write().await.reset();
                            }
                        }
                    }
                    state.increment_seqnum();
                }
                sig = fwd_rx.recv() => match sig {
                    Some(Signal::NewTarget(ip)) => {
                        let mut state = send_state.send().write().await;
                        state.reset_seqnum();
                        state.disable();
                        send_state.recv().write().await.reset();
                        udp_tx = UdpSocket::bind("0.0.0.0:0")
                            .await
                            .expect("Failed to bind tx socket");
                        udp_tx
                            .connect(&format!("{}:1110", &ip))
                            .await
                            .expect("Failed to connect to new target");
                        backoff.reset();
                    }
                    Some(Signal::NewMode(DsMode::Simulation)) => {
                        let mut state = send_state.send().write().await;
                        state.reset_seqnum();
                        state.disable();
                        send_state.recv().write().await.reset();
                        udp_tx
                            .connect("127.0.0.1:1110")
                            .await
                            .expect("Failed to connect to simulator socket");
                        backoff.reset();
                    }
                    _ => {}
                },
            }
        }
    });

    // I need the tokio extension for this, the futures extension to split codecs, and I can't import them both
    // Thanks for coordinating trait names to make using both nicely impossible

    let mut connected = true;
    loop {
        tokio::select! {
            packet = timeout(Duration::from_secs(2), udp_rx.next()) => match packet {
                Ok(timeout_result) => match timeout_result {
                    Some(Ok(packet)) => {
                        if !connected {
                            connected = true;
                        }
                        let (packet, _): (UdpResponsePacket, _) = packet;
                        let mut _state = state.recv().write().await;

                        if packet.need_date {
                            let local = Utc::now();
                            let micros = local.naive_utc().and_utc().timestamp_subsec_micros();
                            let second = local.time().second() as u8;
                            let minute = local.time().minute() as u8;
                            let hour = local.time().hour() as u8;
                            let day = local.date_naive().day() as u8;
                            let month = local.date_naive().month0() as u8;
                            let year = (local.date_naive().year() - 1900) as u8;
                            let tag = DTTag::new(micros, second, minute, hour, day, month, year);
                            state.send().write().await.queue_udp(UdpTag::DateTime(tag));
                        }

                        if !tcp_connected {
                            let (tx, rx) = unbounded_channel::<Signal>();
                            tcp_tx = Some(tx);
                            let mode = *state.send().read().await.ds_mode();
                            if mode == DsMode::Normal {
                                tokio::spawn(tcp_conn(state.clone(), target_ip.clone(), rx));
                            } else {
                                tokio::spawn(tcp_conn(state.clone(), "127.0.0.1".to_string(), rx));
                            }
                            tcp_connected = true;
                        }

                        if packet.status.emergency_stopped() {
                            let mut send = state.send().write().await;
                            if !send.estopped() {
                                send.estop();
                            }
                        }

                        _state.set_trace(packet.trace);
                        _state.set_battery_voltage(packet.battery);
                    }
                    Some(Err(e)) => println!("Error decoding packet: {:?}", e),
                    None => break,
                },
                Err(_) => {
                    if connected {
                        println!("RIO disconnected");
                        state.recv().write().await.reset();
                        connected = false;
                    }
                }
            },
            sig = rx.recv() => match sig {
                Some(Signal::Disconnect) => return Ok(()),
                Some(Signal::NewTarget(ref target)) => {
                    if let Some(ref tcp_tx) = tcp_tx {
                        let _ = tcp_tx.send(Signal::Disconnect);
                        tcp_connected = false;
                    }

                    target_ip = target.clone();

                    fwd_tx.send(sig.unwrap())?;
                }
                Some(Signal::NewMode(mode)) => {
                    let current_mode = *state.send().read().await.ds_mode();
                    if mode != current_mode {
                        if let Some(ref tcp_tx) = tcp_tx {
                            let _ = tcp_tx.send(Signal::Disconnect);
                            tcp_connected = false;
                        }
                        state.send().write().await.set_ds_mode(mode);
                        if mode == DsMode::Normal {
                            println!("Exiting simulation mode");
                            fwd_tx.send(Signal::NewTarget(target_ip.clone()))?;
                        }
                        fwd_tx.send(sig.unwrap())?;
                    }
                }
                None => break,
            },
        }
    }
    Ok(())
}

/// tokio task for all TCP communications
///
/// This task will decode incoming TCP packets, and call the tcp consumer defined in `state` if it exists.
/// It will also accept packets to send from a channel set in `state`, for tasks such as defining game data.
pub(crate) async fn tcp_conn(
    state: Arc<DsState>,
    target_ip: String,
    mut rx: UnboundedReceiver<Signal>,
) -> Result<()> {
    let conn = TcpStream::connect(&format!("{}:1740", target_ip)).await?;
    let codec = DsTcpCodec.framed(conn);
    let (mut codec_tx, mut codec_rx) = codec.split();

    let (tag_tx, mut tag_rx) = unbounded_channel::<TcpTag>();
    state.tcp().write().await.set_tcp_tx(Some(tag_tx));

    let state = state.tcp();
    loop {
        tokio::select! {
            packet = codec_rx.next() => match packet {
                Some(packet) => {
                    if let Ok(packet) = packet {
                        let mut state = state.write().await;
                        if let Some(ref mut consumer) = state.tcp_consumer {
                            consumer(packet);
                        }
                    }
                },
                None => break,
            },
            _ = rx.recv() => {
                state.write().await.set_tcp_tx(None);
            },
            tag = tag_rx.recv() => match tag {
                Some(tag) => {
                    let _ = codec_tx.send(tag).await;
                },
                None => break,
            }
        }
    }
    Ok(())
}

pub(crate) async fn sim_conn(tx: UnboundedSender<Signal>) -> Result<()> {
    use tokio::time::timeout;
    const SOCK_TIMEOUT: Duration = Duration::from_millis(250);

    let sock = UdpSocket::bind("127.0.0.1:1135").await?;
    let mut buf = [0];
    let mut opmode = DsMode::Normal;
    loop {
        match timeout(SOCK_TIMEOUT, sock.recv(&mut buf[..])).await {
            Ok(_) => {
                if opmode != DsMode::Simulation {
                    opmode = DsMode::Simulation;
                    tx.send(Signal::NewMode(DsMode::Simulation)).unwrap();
                }
            }
            Err(_) => {
                if opmode != DsMode::Normal {
                    opmode = DsMode::Normal;
                    tx.send(Signal::NewMode(DsMode::Normal)).unwrap();
                }
            }
        }
    }
}
