use std::cmp::Ordering;

// RTP
// Note: bit ordering assuming big-endian byte extraction
// -----------------------------------------------------------------------------------------------------------------------------
// first_byte (u8)
// Bits 6-7: Version (should be 0b10, else the packet can't be parsed)
// Bit 5: Padding (true means there are trailing padding bytes at the end of the payload) 
// Bit 4: Extension (true means there's an extension header between byte 12 and the payload)
// Bits 0-3: CRSC count (number of 4-byte contributing source IDs following the main header, usually 0 in broadcast)
// -----------------------------------------------------------------------------------------------------------------------------
// second_byte (u8)
// Bit 7: Marker (true means it's the last packet of a single frame)
// Bits 0-6: Payload type (Dynamic payload identifier assigned to ST 2110, typically 96 to 127)
// -----------------------------------------------------------------------------------------------------------------------------
// sequence_number (u16)
// Bits 0-15: Number of packet (to detect packet loss)
// -----------------------------------------------------------------------------------------------------------------------------
// timestamp (u32)
// Bits 0-31: Presentation clock (same video frame, same time stamp)
// -----------------------------------------------------------------------------------------------------------------------------
// ssrc (u32)
// Bits 0-31: Stream identifier (which camera sent this packet)
#[derive(Clone, Copy, Debug)]
pub struct RTP {
    first_byte: u8,
    second_byte: u8,
    sequence_number: u16,
    timestamp: u32,
    ssrc: u32,
} 

// Packets with the same sequence_number shouldn't occurr, this is just a safety net
impl PartialEq for RTP {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.sequence_number == other.sequence_number
    }
}

impl Eq for RTP {}

// This makes it easier to ensure packets are being received in the right order
impl PartialOrd for RTP {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RTP {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        let modular_sub = self.sequence_number.wrapping_sub(other.sequence_number) as i16;
        modular_sub.cmp(&0)
    }
}

impl RTP {

    // Returns an RTP struct from a packet
    // Note: must ensure packet is valid, meaning at least 12 bytes long
    #[inline]
    pub fn parse(packet: &[u8]) -> Self {
        let fb = packet[0];
        let sb = packet[1];
        let sn = u16::from_be_bytes(packet[2..4].try_into().unwrap());
        let t = u32::from_be_bytes(packet[4..8].try_into().unwrap());
        let s = u32::from_be_bytes(packet[8..12].try_into().unwrap());
        Self { first_byte: fb, second_byte: sb, sequence_number: sn, timestamp: t, ssrc: s }
    }

    // Version getter
    #[inline]
    pub fn version(&self) -> u8 {
        self.first_byte >> 6
    }

    // Padding getter
    #[inline]
    pub fn padding(&self) -> bool {
        self.first_byte >> 5 & 0b1 == 1
    }

    // Extension getter
    #[inline]
    pub fn extension(&self) -> bool {
        self.first_byte >> 4 & 0b1 == 1
    }

    // CRSC count getter
    #[inline]
    pub fn crsc_count(&self) -> u8 {
        self.first_byte & 0b1111
    }

    // Marker getter
    #[inline]
    pub fn marker(&self) -> bool {
        self.second_byte >> 7 == 1
    }

    // Payload type getter
    #[inline]
    pub fn payload_type(&self) -> u8 {
        self.second_byte & 0b1111111
    }

    // Sequence number getter
    #[inline] 
    pub fn sequence_number(&self) -> u16 {
        self.sequence_number
    }

    // Timestamp getter
    #[inline]
    pub fn timestamp(&self) -> u32 {
        self.timestamp
    }

    // SSRC getter
    #[inline]
    pub fn ssrc(&self) -> u32 {
        self.ssrc
    }

    // Ensuring version is supported
    #[inline]
    pub fn valid_version(&self) -> bool {
        self.version() == 0b10
    }

    // Ensuring two packets relate to the same frame
    #[inline]
    pub fn same_frame(&self, other: &Self) -> bool {
        self.timestamp == other.timestamp
    }

}