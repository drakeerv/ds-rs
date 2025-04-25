use crate::Result;
use crate::ext::BufExt;
use crate::util::InboundTag;
use bytes::Buf;

macro_rules! gen_stub_tags {
    ($($struct_name:ident : $num_bytes:expr),*) => {
        $(
        pub(crate) struct $struct_name {
            _data: [u8; $num_bytes]
        }

        impl InboundTag for $struct_name {
            fn chomp(buf: &mut impl Buf) -> Result<Self> {
                let mut _data = [0; $num_bytes];

                for i in 0..$num_bytes {
                    _data[i] = buf.read_u8()?;
                }

                Ok($struct_name { _data })
            }
        }
        )*
    }
}

// UDP tags should be eaten to ensure the pipe doesn't get clogged, but for now proper structs aren't implemented.
gen_stub_tags!(PDPLog : 25, JoystickOutput : 8, DiskInfo : 4, CPUInfo : 20, RAMInfo : 8, Unknown : 9, CANMetrics : 14);


bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Status: u8 {
        const ESTOP = 0b1000_0000;
        const BROWNOUT = 0b0001_0000;
        const CODE_START = 0b0000_1000;
        const ENABLED = 0b0000_0100;

        // Mode flags
        const TELEOP = 0b00;
        const TEST = 0b01;
        const AUTO = 0b10;
    }
}

impl Status {
    #[inline(always)]
    pub const fn is_browning_out(self) -> bool {
        self.contains(Status::BROWNOUT)
    }

    #[inline(always)]
    pub const fn emergency_stopped(self) -> bool {
        self.contains(Status::ESTOP)
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Trace: u8 {
        const ROBOT_CODE = 0b0010_0000;
        const IS_ROBORIO = 0b0001_0000;
        const TEST_MODE = 0b0000_1000;
        const AUTONOMOUS = 0b0000_0100;
        const TELEOP = 0b0000_0010;
        const DISABLED = 0b0000_0001;
    }
}

macro_rules! gen_trace_methods {
    ($($func_name:ident => $flag_name:expr),+) => {
        impl Trace {
            $(
            #[inline(always)]
            pub const fn $func_name(self) -> bool {
                self.contains($flag_name)
            }
            )+
        }
    }
}

gen_trace_methods!(is_autonomous => Trace::AUTONOMOUS, is_teleop => Trace::TELEOP, is_disabled => Trace::DISABLED,
                   is_test => Trace::TEST_MODE, is_code_started => Trace::ROBOT_CODE, is_connected => Trace::IS_ROBORIO);
