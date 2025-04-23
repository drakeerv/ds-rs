//! This module contains various tags that can be attached to the outbound UDP packet
//! The `Tag` trait contains the core logic, and is inherited by structs with specific roles

use bytes::{BufMut, Bytes, BytesMut};

// Assuming this helper still exists and returns Vec<u8> or preferably Bytes
use crate::util::to_u8_vec;

/// Enum wrapping possible outgoing UDP tags
#[derive(Clone, Debug)]
pub enum UdpTag {
    /// Tag sent to inform user code of the time left in the current mode
    Countdown(Countdown),
    /// Tag sent to provide joystick input to the user code
    Joysticks(Joysticks),
    /// Tag sent to update the roboRIO system clock to match that of the driver station
    DateTime(DateTime),
    /// Tag sent to update the roboRIO timezone. Sent alongside the DateTime tag
    Timezone(Timezone),
}

/// Represents an outgoing UDP tag
pub(crate) trait Tag: Send {
    /// Returns the unique ID byte for this tag type.
    fn id(&self) -> u8;

    /// Returns the serialized data payload for this tag as immutable Bytes.
    fn data(&self) -> Bytes;

    /// Constructs the final tag bytes including the length prefix and ID.
    /// Format: Length (u8) | ID (u8) | Data (...)
    fn construct(&self) -> Bytes {
        let id_byte = self.id();
        let data_bytes = self.data();
        let data_len = data_bytes.len();

        let payload_len = 1 + data_len;

        assert!(
            payload_len <= u8::MAX as usize,
            "Tag payload too large for u8 length field"
        );

        let total_len = 1 + payload_len;
        let mut buf = BytesMut::with_capacity(total_len);

        buf.put_u8(payload_len as u8);
        buf.put_u8(id_byte);
        buf.put(data_bytes);

        buf.freeze()
    }
}

/// Tag containing the time remaining in the current mode
#[derive(Clone, Debug)]
pub struct Countdown {
    seconds_remaining: f32,
}

impl Countdown {
    #[inline(always)]
    pub const fn new(seconds: f32) -> Countdown {
        Countdown {
            seconds_remaining: seconds,
        }
    }
}

impl Tag for Countdown {
    #[inline(always)]
    fn id(&self) -> u8 {
        0x07
    }

    fn data(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(4);
        buf.put_f32(self.seconds_remaining);
        buf.freeze()
    }
}

/// Tag containing values from joysticks
#[derive(Clone, Debug)]
pub struct Joysticks {
    axes: Vec<i8>,
    buttons: Vec<bool>,
    povs: Vec<i16>,
}

impl Joysticks {
    #[inline(always)]
    pub fn new(axes: Vec<i8>, buttons: Vec<bool>, povs: Vec<i16>) -> Joysticks {
        Joysticks {
            axes,
            buttons,
            povs,
        }
    }
}

impl Tag for Joysticks {
    #[inline(always)]
    fn id(&self) -> u8 {
        0x0c
    }

    fn data(&self) -> Bytes {
        let buttons_packed = to_u8_vec(&self.buttons);
        let capacity = 1 // axes count
                     + self.axes.len() // Each i8 is 1 byte
                     + 1 // button count
                     + buttons_packed.len()
                     + 1 // pov count
                     + (self.povs.len() * 2); // Each i16 is 2 bytes
        let mut buf = BytesMut::with_capacity(capacity);

        assert!(
            self.axes.len() <= u8::MAX as usize,
            "Too many axes for u8 count"
        );
        buf.put_u8(self.axes.len() as u8);

        for axis in &self.axes {
            buf.put_i8(*axis);
        }

        assert!(
            self.buttons.len() <= u8::MAX as usize,
            "Too many buttons for u8 count"
        );
        buf.put_u8(self.buttons.len() as u8);
        buf.extend_from_slice(&buttons_packed);

        assert!(
            self.povs.len() <= u8::MAX as usize,
            "Too many POVs for u8 count"
        );
        buf.put_u8(self.povs.len() as u8);
        for pov in &self.povs {
            buf.put_i16(*pov);
        }

        buf.freeze()
    }
}

/// Tag containing the current date and time in UTC
#[derive(Clone, Debug)]
pub struct DateTime {
    micros: u32,
    second: u8,
    minute: u8,
    hour: u8,
    day: u8,
    month: u8,
    year: u8,
}

impl DateTime {
    #[inline(always)]
    pub fn new(
        micros: u32,
        second: u8,
        minute: u8,
        hour: u8,
        day: u8,
        month: u8,
        year: u8,
    ) -> DateTime {
        DateTime {
            micros,
            second,
            minute,
            hour,
            day,
            month,
            year,
        }
    }
}

impl Tag for DateTime {
    #[inline(always)]
    fn id(&self) -> u8 {
        0x0f
    }

    fn data(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(4 + 1 + 1 + 1 + 1 + 1 + 1);
        buf.put_u32(self.micros);
        buf.put_u8(self.second);
        buf.put_u8(self.minute);
        buf.put_u8(self.hour);
        buf.put_u8(self.day);
        buf.put_u8(self.month);
        buf.put_u8(self.year);
        buf.freeze()
    }
}

/// Tag containing the current timezone of the RIO
#[derive(Clone, Debug)]
pub struct Timezone {
    tz: String,
}

impl Timezone {
    #[inline(always)]
    pub fn new(tz: impl Into<String>) -> Timezone {
        Timezone { tz: tz.into() }
    }
}

impl Tag for Timezone {
    #[inline(always)]
    fn id(&self) -> u8 {
        0x10
    }

    fn data(&self) -> Bytes {
        Bytes::from(self.tz.clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn verify_countdown_format() {
        let countdown = Countdown::new(2.0f32);
        let buf = countdown.construct();
        assert_eq!(buf.as_ref(), &[0x05, 0x07, 0x40, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn verify_joysticks_format() {
        let joysticks = Joysticks::new(
            vec![-128, 0, 127],
            vec![true, false, true, false, false, false, false, false, true],
            vec![0, 18000],
        );
        let buf = joysticks.construct();
        assert_eq!(
            buf.as_ref(),
            &[
                0x0D, 0x0c, 0x03, 0x80, 0x00, 0x7F, 0x09, 0x05, 0x01, 0x02, 0x00, 0x00, 0x46, 0x50
            ]
        );
    }

    #[test]
    fn verify_datetime_format() {
        let dt = DateTime::new(123456, 30, 55, 17, 23, 4, 124);
        let buf = dt.construct();
        assert_eq!(
            buf.as_ref(),
            &[
                0x0B, 0x0f, 0x00, 0x01, 0xE2, 0x40, 0x1E, 0x37, 0x11, 0x17, 0x04, 0x7C
            ]
        );
    }

    #[test]
    fn verify_timezone_format() {
        let tz = Timezone::new("UTC");
        let buf = tz.construct();
        assert_eq!(buf.as_ref(), &[0x04, 0x10, 0x55, 0x54, 0x43]);
    }
}
