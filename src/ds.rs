use anyhow::bail;

mod conn;
pub(crate) mod state;

use self::conn::*;
use self::state::*;

use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::proto::tcp::outbound::{GameData, TcpTag};
use crate::proto::udp::inbound::types::Trace;
use crate::proto::udp::outbound::types::tags::UdpTag;
use crate::proto::udp::outbound::types::*;
use crate::util::ip_from_team_number;
use crate::{Result, TcpPacket};

/// Represents a connection to the roboRIO acting as a driver station
///
/// This struct will contain relevant functions to update the state of the robot,
/// and also manages the threads that manage network connections and joysticks
pub struct DriverStation {
    thread_tx: UnboundedSender<Signal>,
    team_number: u16,
    state: Arc<DsState>,
}

impl DriverStation {
    /// Creates a new driver station with the given team number and alliance
    ///
    /// This driver station will attempt to connect to a roboRIO at 10.TE.AM.2,
    /// if the roboRIO is at a different ip, use [new] and specify the ip directly.
    pub async fn new_team(team_number: u16, alliance: Alliance) -> DriverStation {
        Self::new(&ip_from_team_number(team_number), alliance, team_number).await
    }

    /// Creates a new driver station for the given alliance station and team number
    /// Connects to the roborio at `ip`. To infer the ip from team_number, use `new_team` instead.
    pub async fn new(ip: &str, alliance: Alliance, team_number: u16) -> DriverStation {
        // Channels to communicate to the threads that make up the application, used to break out of infinite loops when the struct is dropped
        let (tx, rx) = unbounded_channel::<Signal>();

        // Global state of the driver station
        let state = Arc::new(DsState::new(alliance));

        // Thread containing UDP sockets communicating with the roboRIO
        let udp_state = state.clone();
        let udp_ip = ip.to_owned();

        let sim_tx = tx.clone();
        tokio::spawn(async {
            sim_conn(sim_tx).await.unwrap();
        });
        udp_conn(udp_state, udp_ip, rx)
            .await
            .expect("Error with udp connection");

        DriverStation {
            thread_tx: tx,
            state,
            team_number,
        }
    }

    /// Provides a closure that will be called when constructing outbound packets to append joystick values
    pub async fn set_joystick_supplier(
        &mut self,
        supplier: impl Fn() -> Vec<Vec<JoystickValue>> + Send + Sync + 'static,
    ) {
        self.state
            .send()
            .write()
            .await
            .set_joystick_supplier(supplier);
    }

    /// Provides a closure that will be called when TCP packets are received from the roboRIO
    ///
    /// Example usage: Logging all stdout messages from robot code.
    pub async fn set_tcp_consumer(
        &mut self,
        consumer: impl FnMut(TcpPacket) + Send + Sync + 'static,
    ) {
        self.state.tcp().write().await.set_tcp_consumer(consumer);
    }

    /// Changes the alliance for the given `DriverStation`
    pub async fn set_alliance(&mut self, alliance: Alliance) {
        self.state.send().write().await.set_alliance(alliance);
    }

    /// Changes the given `mode` the robot will be in
    pub async fn set_mode(&mut self, mode: Mode) {
        self.state.send().write().await.set_mode(mode);
    }

    pub async fn ds_mode(&self) -> DsMode {
        self.state.send().read().await.ds_mode()
    }

    /// Changes the team number of this driver station, as well as the ip the driver station will attempt to connect to.
    /// The ip of the new roboRIO target is 10.TE.AM.2
    pub fn set_team_number(&mut self, team_number: u16) {
        self.team_number = team_number;
        self.thread_tx
            .send(Signal::NewTarget(ip_from_team_number(team_number)))
            .unwrap();
    }

    pub fn set_use_usb(&mut self, use_usb: bool) {
        if use_usb {
            self.thread_tx
                .send(Signal::NewTarget("172.22.11.2".to_string()))
                .unwrap();
        } else {
            self.thread_tx
                .send(Signal::NewTarget(ip_from_team_number(self.team_number)))
                .unwrap();
        }
    }

    #[inline(always)]
    pub const fn team_number(&self) -> u16 {
        self.team_number
    }

    /// Sets the game specific message sent to the robot, and used during the autonomous period
    pub async fn set_game_specific_message(&mut self, message: &str) -> Result<()> {
        if message.len() != 3 {
            bail!("Message should be 3 characters long");
        }

        let _ = self
            .state
            .tcp()
            .write()
            .await
            .queue_tcp(TcpTag::GameData(GameData {
                gsm: message.to_string(),
            }));
        Ok(())
    }

    /// Returns the current mode of the robot
    pub async fn mode(&self) -> Mode {
        self.state.send().read().await.mode()
    }

    /// Enables outputs on the robot
    pub async fn enable(&mut self) {
        self.state.send().write().await.enable();
    }

    /// Instructs the roboRIO to restart robot code
    pub async fn restart_code(&mut self) {
        self.state
            .send()
            .write()
            .await
            .request(Request::RESTART_CODE);
    }

    /// Instructs the roboRIO to reboot
    pub async fn restart_roborio(&mut self) {
        self.state
            .send()
            .write()
            .await
            .request(Request::REBOOT_ROBORIO);
    }

    /// Returns whether the robot is currently enabled
    pub async fn enabled(&self) -> bool {
        self.state.send().read().await.enabled()
    }

    /// Returns the last received Trace from the robot
    pub async fn trace(&self) -> Trace {
        self.state.recv().read().await.trace()
    }

    /// Returns the last received battery voltage from the robot
    pub async fn battery_voltage(&self) -> f32 {
        self.state.recv().read().await.battery_voltage()
    }

    /// Queues a UDP tag to be transmitted with the next outbound packet to the roboRIO
    pub async fn queue_udp(&mut self, udp_tag: UdpTag) {
        self.state.send().write().await.queue_udp(udp_tag);
    }

    /// Returns a Vec of the current contents of the UDP queue
    pub async fn udp_queue(&self) -> Vec<UdpTag> {
        self.state.send().read().await.pending_udp().clone()
    }

    /// Queues a TCP tag to be transmitted to the roboRIO
    pub async fn queue_tcp(&mut self, tcp_tag: TcpTag) {
        let _ = self.state.tcp().write().await.queue_tcp(tcp_tag);
    }

    /// Disables outputs on the robot and disallows enabling it until the code is restarted.
    pub async fn estop(&mut self) {
        self.state.send().write().await.estop();
    }

    /// Returns whether the robot is currently E-stopped
    pub async fn estopped(&self) -> bool {
        self.state.send().read().await.estopped()
    }

    /// Disables outputs on the robot
    pub async fn disable(&mut self) {
        self.state.send().write().await.disable();
    }
}

/// Enum representing a value from a Joystick to be transmitted to the roboRIO
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum JoystickValue {
    /// Represents an axis value to be sent to the roboRIO
    ///
    /// `value` should range from `-1.0..=1.0`, or `0.0..=1.0` if the axis is a trigger
    Axis { id: u8, value: f32 },
    /// Represents a button value to be sent to the roboRIO
    Button { id: u8, pressed: bool },
    /// Represents a POV, or D-pad value to be sent to the roboRIO
    POV { id: u8, angle: i16 },
}

impl JoystickValue {
    #[inline(always)]
    pub const fn id(self) -> u8 {
        match self {
            JoystickValue::Axis { id, .. } => id,
            JoystickValue::Button { id, .. } => id,
            JoystickValue::POV { id, .. } => id,
        }
    }

    #[inline(always)]
    pub const fn is_axis(self) -> bool {
        matches!(self, JoystickValue::Axis { .. })
    }

    #[inline(always)]
    pub const fn is_button(self) -> bool {
        matches!(self, JoystickValue::Button { .. })
    }

    #[inline(always)]
    pub const fn is_pov(self) -> bool {
        matches!(self, JoystickValue::POV { .. })
    }
}

impl Drop for DriverStation {
    fn drop(&mut self) {
        // When this struct is dropped the threads that we spawned should be stopped otherwise we're leaking
        let _ = self.thread_tx.send(Signal::Disconnect);
    }
}

#[derive(Debug)]
pub(crate) enum Signal {
    Disconnect,
    NewTarget(String),
    NewMode(DsMode),
}
