use crate::srd::SRD;

// Payload
// Note: bit ordering assuming big-endian byte extraction
// -----------------------------------------------------------------------------------------------------------------------------
// extended_sequence_number (u16)
// Bits 0-15: Allows for a bigger range of sequence numbers
// -----------------------------------------------------------------------------------------------------------------------------
// length (u16)
// Bits 0-15: Exact number of pixel data bytes belonging to this scanline chunk
// -----------------------------------------------------------------------------------------------------------------------------
// last_four_bytes (u32)
// Bit 7: Field (should always be 0 for this engine)
// Bits 0-6|8-15: Line number (indicates the line we're treating from 0 to 1079)
// Bit 23: Continuation (if active, this packet's payload includes a line change, meaning there's another 6 byte SRD header)
// Bits 16-22|24-31: Horizontal Offset (starting column along the line from 0 to 1979)
#[derive(Clone, Copy, Debug)]
pub struct Payload {
    extended_sequence_number: u16,
    srd: [SRD; 3],
    count: u8
}

impl Payload {

    // Returns a Payload struct from a packet's payload
    // Note: must ensure packet's payload is valid, meaning at least 2 + 6 * count bytes long
    #[inline]
    pub fn parse(packet_payload: &[u8]) -> Self {
        let esn = u16::from_be_bytes(packet_payload[0..2].try_into().unwrap());
        let mut s = [SRD::empty(); 3];
        s[0] = SRD::parse(&packet_payload[2..]);
        if !s[0].continuation() {
            return Payload { extended_sequence_number: esn, srd: s, count: 1 };
        }
        s[1] = SRD::parse(&packet_payload[8..]);
        if !s[1].continuation() {
            return Payload { extended_sequence_number: esn, srd: s, count: 2 };
        }
        s[2] = SRD::parse(&packet_payload[14..]);
        Payload { extended_sequence_number: esn, srd: s, count: 3 }
    }

    // Srd getter
    #[inline]
    pub fn srd(&self, index: u8) -> SRD {
        if index >= self.count {
            return SRD::empty();
        }
        self.srd[index as usize]
    }

    // Srds getter
    #[inline]
    pub fn srds(&self) -> &[SRD] {
        &self.srd[..self.count as usize]
    }

    // Extended sequence number getter
    #[inline]
    pub fn extended_sequence_number(&self) -> u16 {
        self.extended_sequence_number
    }

    // Length getter
    #[inline]
    pub fn length(&self, index: u8) -> u16 {
        if index >= self.count {
            return 0;
        }
        self.srd[index as usize].length()
    }

    // Total length getter
    #[inline]
    pub fn total_length(&self) -> u16 {
        self.srds().iter().fold(0, |acc, s| acc + s.length())
    }

    // Field getter
    #[inline]
    pub fn field(&self, index: u8) -> bool {
        if index >= self.count {
            return false;
        }
        self.srd[index as usize].field()
    }

    // Line number getter
    #[inline]
    pub fn line_number(&self, index: u8) -> u16 {
        if index >= self.count {
            return 0;
        }
        self.srd[index as usize].line_number()
    }

    // Continuation getter
    #[inline]
    pub fn continuation(&self, index: u8) -> bool {
        if index >= self.count {
            return false;
        }
        self.srd[index as usize].continuation()
    }

    // Horizontal offset getter
    #[inline]
    pub fn horizontal_offset(&self, index: u8) -> u16 {
        if index >= self.count {
            return 0;
        }
        self.srd[index as usize].horizontal_offset()
    }

    // Count getter
    #[inline]
    pub fn count(&self) -> u8 {
        self.count
    }

    // SRD slice begin getter
    #[inline]
    pub fn srd_slice_begin(&self, index: u8) -> u16 {
        if index >= self.count {
            return 0;
        }
        let mut b = 12 + (2 + self.count * 6) as u16;
        for i in 0..index {
            b += self.length(i);
        }
        b
    }

    // SRD slice end getter
    #[inline]
    pub fn srd_slice_end(&self, index: u8) -> u16 {
        self.srd_slice_begin(index) + self.length(index)
    }

    // Grabs the pixel slice relative to a specific SRD header
    #[inline]
    pub fn pixel_slice<'a>(&self, packet_payload: &'a [u8], index: u8) -> &'a [u8] {
        &packet_payload[self.srd_slice_begin(index) as usize..self.srd_slice_end(index) as usize]
    }

}