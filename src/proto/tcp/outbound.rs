use bytes::{BufMut, Bytes, BytesMut};

#[derive(Debug, Clone)]
pub enum TcpTag {
    MatchInfo(MatchInfo),
    GameData(GameData),
}

pub(crate) trait OutgoingTcpTag {
    fn id(&self) -> u8;

    fn data(&self) -> Bytes;

    fn construct(&self) -> Bytes {
        let id = self.id();
        let data = self.data();
        let data_len = data.len();
        let payload_len = 1 + data_len;

        // Check size
        assert!(
            payload_len <= u16::MAX as usize,
            "Payload too large for u16 length"
        );

        let total_len = 2 + payload_len; // Total buffer size = Length field + Payload
        let mut buf = BytesMut::with_capacity(total_len);

        buf.put_u16(payload_len as u16);
        buf.put_u8(id);
        buf.extend_from_slice(&data);

        buf.freeze()
    }
}

#[derive(Debug, Clone)]
pub struct MatchInfo {
    competition: String,
    match_type: MatchType,
}

impl OutgoingTcpTag for MatchInfo {
    #[inline(always)]
    fn id(&self) -> u8 {
        0x07
    }

    fn data(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(1 + self.competition.len() + 1);
        buf.put_u8(self.competition.len() as u8);
        buf.put_slice(self.competition.as_bytes());
        buf.put_u8(self.match_type as u8);
        buf.freeze()
    }
}

#[derive(Debug, Clone)]
pub struct GameData {
    pub gsm: String,
}

impl OutgoingTcpTag for GameData {
    fn id(&self) -> u8 {
        0x0e
    }

    fn data(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(self.gsm.len());
        buf.put_slice(self.gsm.as_bytes());
        buf.freeze()
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
#[allow(unused)]
pub enum MatchType {
    None = 0,
    Practice = 1,
    Qualifications = 2,
    Eliminations = 3,
}
