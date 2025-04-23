pub mod tags;

bitflags! {
    /// bitflag struct for the Control value of the packet
    pub struct Control: u8 {
        const ESTOP = 0b1000_0000;
        const FMS_CONNECTED = 0b0000_1000;
        const ENABLED = 0b0000_0100;

        // Mode flags
        const TELEOP = 0b00;
        const TEST = 0b01;
        const AUTO = 0b10;
    }
}

bitflags! {
    /// bitflags for reboot and code restart requests
    pub struct Request: u8 {
        const REBOOT_ROBORIO = 0b0000_1000;
        const RESTART_CODE = 0b0000_0100;
    }
}

/// Struct abstracting the byte value for alliance colour and position
#[derive(Copy, Clone, Debug)]
pub struct Alliance(pub u8);

impl Alliance {
    /// Creates a new `Alliance` for the given position, on the red alliance
    #[inline(always)]
    pub const fn new_red(position: u8) -> Alliance {
        assert!(position <= 3 && position != 0);

        Alliance(position - 1)
    }

    /// Creates a new `Alliance` for the given position, on the blue alliance
    #[inline(always)]
    pub const fn new_blue(position: u8) -> Alliance {
        assert!(position <= 3 && position != 0);

        Alliance(position + 2)
    }

    /// Returns true if `self` is on the red alliance, false otherwise
    ///
    /// !is_red() implies is_blue()
    #[inline(always)]
    pub const fn is_red(self) -> bool {
        self.0 < 3
    }

    /// Returns true if `self` is on the blue alliance, false otherwise
    #[inline(always)]
    pub const fn is_blue(self) -> bool {
        !self.is_red()
    }

    /// Returns the alliance station position for `self`
    #[inline(always)]
    pub const fn position(self) -> u8 {
        (self.0 % 3) + 1
    }
}
