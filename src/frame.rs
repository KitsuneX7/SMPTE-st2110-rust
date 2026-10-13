use std::fs::File;
use std::io::{BufWriter, Write};

// Streaming in 1920x1080p
// Each pixel is 40 bits (2.5 bytes): Cb0, Y0, Cr0, Y1
// Colors (Cb and Cr) are shared, brightness (Y) is different
pub const FRAME_WIDTH: usize = 1920;
pub const FRAME_HEIGHT: usize = 1080;
pub const BYTES_PER_LINE: usize = 4800; // 1920 * 2.5
pub const FRAME_SIZE: usize = FRAME_HEIGHT * BYTES_PER_LINE;

// Helper method for color conversion for ppm
#[inline]
fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> (u8, u8, u8) {
    let y = y as f32;
    let cb = cb as f32 - 128.0;
    let cr = cr as f32 - 128.0;
    let r = (y + 1.402 * cr).clamp(0.0, 255.0) as u8;
    let g = (y - 0.344136 * cb - 0.714136 * cr).clamp(0.0, 255.0) as u8;
    let b = (y + 1.772 * cb).clamp(0.0, 255.0) as u8;
    (r, g, b)
}

// Frame
// -----------------------------------------------------------------------------------------------------------------------------
// buffer (Box<[u8]>): Heap-allocated 5Mb buffer to store the streamed bytes
// timestamp (u32): Records the shared timestamp from all packets that make up this frame
// packets_received (u32): This one explains itself
pub struct Frame {
    buffer: Box<[u8]>,
    timestamp: u32,
    packets_received: u32,
}

impl Frame {

    // New method (allocates the buffer once on the heap, records a dummy timestamp, and zeroes packets received)
    pub fn new() -> Self {
        Self { buffer: vec![0u8; FRAME_SIZE].into_boxed_slice(), timestamp: 0, packets_received: 0 }
    }

    // Reset method (doesn't require resetting the buffer since it'll be filled again anyway)
    #[inline]
    pub fn reset(&mut self, t: u32) {
        self.timestamp = t;
        self.packets_received = 0;
    }

    // Buffer getter
    #[inline]
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    // Timestamp getter
    #[inline]
    pub fn timestamp(&self) -> u32 {
        self.timestamp
    }

    // Packets received getter
    #[inline]
    pub fn packets_received(&self) -> u32 {
        self.packets_received
    }

    // Writes a part (or the entirety of) a line sent over by a packet
    #[inline]
    pub fn write_pixel_slice(&mut self, line: u16, offset: u16, packet_payload: &[u8]) {
        let begin = (line as usize) * BYTES_PER_LINE + ((offset as usize) * 5 >> 1);
        let end = begin + packet_payload.len();
        if end > FRAME_SIZE {
            return;
        }
        self.buffer[begin..end].copy_from_slice(packet_payload);
    }

    // Packet number recorder
    #[inline]
    pub fn record_packet(&mut self) {
        self.packets_received += 1;
    }

    pub fn save_ppm(&self, path: &str) -> std::io::Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);

        // Netpbm P6 header: binary RGB, width, height, max color value
        write!(writer, "P6\n{} {}\n255\n", FRAME_WIDTH, FRAME_HEIGHT)?;

        // Each 5 bytes contains 2 pixels (40-bit pgroup)
        for pgroup in self.buffer.chunks_exact(5) {
            let cb = pgroup[0];
            let y0 = ((pgroup[1] & 0x3F) << 2) | (pgroup[2] >> 6);
            let cr = ((pgroup[2] & 0x0F) << 4) | (pgroup[3] >> 4);
            let y1 = ((pgroup[3] & 0x03) << 6) | (pgroup[4] >> 2);

            let (r0, g0, b0) = ycbcr_to_rgb(y0, cb, cr);
            let (r1, g1, b1) = ycbcr_to_rgb(y1, cb, cr);

            writer.write_all(&[r0, g0, b0, r1, g1, b1])?;
        }

        writer.flush()?;
        Ok(())
    }

}