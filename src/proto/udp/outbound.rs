pub mod types;

use self::types::*;
use bytes::{BufMut, Bytes, BytesMut};

/// UDP control packet to send to the roboRIO
pub struct UdpControlPacket {
    pub(crate) seqnum: u16,
    pub(crate) control: Control,
    pub(crate) request: Option<Request>,
    pub(crate) alliance: Alliance,
    pub(crate) tags: Vec<Box<dyn Tag>>,
}

impl UdpControlPacket {
    /// Encodes the current state of the packet into a vec to send to the roboRIO
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(2 + 1 + 1 + 1 + 1);
        buf.put_u16(self.seqnum);
        buf.put_u8(0x01);
        buf.put_u8(self.control.bits());
        buf.put_u8(if let Some(ref req) = self.request {
            req.bits()
        } else {
            0
        });
        buf.put_u8(self.alliance.0);

        for tag in self.tags.iter() {
            buf.extend(tag.construct());
        }

        buf.freeze()

        // let mut buf = vec![];
        // buf.write_u16::<BigEndian>(self.seqnum).unwrap();
        // buf.push(0x01); // comm version
        // buf.push(self.control.bits());
        // match &self.request {
        //     Some(req) => buf.push(req.bits()),
        //     None => buf.push(0),
        // }

        // buf.push(self.alliance.0);

        // for tag in &self.tags {
        //     buf.extend(tag.construct());
        // }

        // buf
    }
}
