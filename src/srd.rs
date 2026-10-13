// SRD
// Note: bit ordering assuming big-endian byte extraction
// -----------------------------------------------------------------------------------------------------------------------------
// length (u16)
// Bits 0-15: Exact number of pixel data bytes belonging to this scanline chunk
// -----------------------------------------------------------------------------------------------------------------------------
// last_four_bytes (u32)
// Bit 7: Field (should always be 0 for this engine)
// Bits 0-6|8-15: Line number (indicates the line we're treating from 0 to 1079)
// Bit 23: Continuation (if active, this packet includes a line change, meaning there's another 6 byte SRD header)
// Bits 16-22|24-31: Horizontal Offset (starting column along the line from 0 to 1979)
#[derive(Clone, Copy, Debug)]
pub struct SRD {
    length: u16,
    field_line_number: u16,
    continuation_horizontal_offset: u16
} 

impl SRD {

    // Creates an empty Payload struct
    #[inline]
    pub fn empty() -> Self {
        Self { length: 0, field_line_number: 0, continuation_horizontal_offset: 0 }
    }

    // Returns a Payload struct from a packet
    // Note: must ensure packet is valid, meaning at least 20 bytes long
    #[inline]
    pub fn parse(packet: &[u8]) -> Self {
        let l = u16::from_be_bytes(packet[0..2].try_into().unwrap());
        let fln = u16::from_be_bytes(packet[2..4].try_into().unwrap());
        let cho = u16::from_be_bytes(packet[4..6].try_into().unwrap());
        Self { length: l, field_line_number: fln, continuation_horizontal_offset: cho }
    }

    // Length getter
    #[inline]
    pub fn length(&self) -> u16 {
        self.length
    }

    // Field getter
    #[inline]
    pub fn field(&self) -> bool {
        self.field_line_number >> 15 == 1
    }

    // Line number getter
    #[inline]
    pub fn line_number(&self) -> u16 {
        self.field_line_number & 0x7FFF
    }

    // Continuation getter
    #[inline]
    pub fn continuation(&self) -> bool {
        self.continuation_horizontal_offset >> 15 == 1
    }

    // Horizontal offset getter
    #[inline]
    pub fn horizontal_offset(&self) -> u16 {
        self.continuation_horizontal_offset & 0x7FFF
    }

}