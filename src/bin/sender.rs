use std::net::UdpSocket;
use std::thread;
use std::time::{Duration, Instant};

const WIDTH: usize = 1920;
const HEIGHT: usize = 1080;
const PIXELS_PER_PACKET: usize = 480; // 240 pgroups (fits in standard MTU)
const BYTES_PER_PACKET_PAYLOAD: usize = (PIXELS_PER_PACKET / 2) * 5; // 1200 bytes

// Standard SMPTE 75% Color Bars in 10-bit (Y, Cb, Cr)
const COLOR_BARS: [(u16, u16, u16); 8] = [
    (721, 512, 512), // White
    (646, 176, 567), // Yellow
    (525, 589, 171), // Cyan
    (450, 253, 226), // Green
    (335, 771, 798), // Magenta
    (260, 435, 853), // Red
    (139, 848, 457), // Blue
    (64,  512, 512), // Black
];

fn main() -> std::io::Result<()> {
    // Bind to an ephemeral port
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let target_addr = "127.0.0.1:5004";
    let frame_duration = Duration::from_micros(16_666); // 60.0 fps timing
    let mut sequence_number = 1u16;
    let mut timestamp = 90000u32;
    let mut packet_buffer = [0u8; 1400];
    println!("Transmitting 10-bit SMPTE ST 2110-20 stream to {target_addr}...");
    for frame_idx in 0..5 {
        let frame_start = Instant::now();
        for line in 0..HEIGHT as u16 {
            let mut h_offset = 0u16;
            while (h_offset as usize) < WIDTH {
                // Check if this is the final packet of the entire video frame
                let is_last_packet = (line as usize == HEIGHT - 1) && ((h_offset as usize + PIXELS_PER_PACKET) >= WIDTH);
                // --- 1. RTP Header (12 bytes) ---
                let marker_bit = if is_last_packet { 0x80 } else { 0x00 };
                packet_buffer[0] = 0x80; // V=2
                packet_buffer[1] = 96 | marker_bit; // PT=96 (Dynamic)
                packet_buffer[2..4].copy_from_slice(&sequence_number.to_be_bytes());
                packet_buffer[4..8].copy_from_slice(&timestamp.to_be_bytes());
                packet_buffer[8..12].copy_from_slice(&0x12345678u32.to_be_bytes()); // SSRC
                // --- 2. Payload Header: ESN (2 bytes) + 1 SRD Header (6 bytes) ---
                packet_buffer[12..14].copy_from_slice(&0x0000u16.to_be_bytes()); // ESN
                packet_buffer[14..16].copy_from_slice(&(BYTES_PER_PACKET_PAYLOAD as u16).to_be_bytes());
                packet_buffer[16..18].copy_from_slice(&line.to_be_bytes());
                packet_buffer[18..20].copy_from_slice(&h_offset.to_be_bytes());
                // --- 3. Pixel Data: Pack 10-bit samples into 5-byte pgroups ---
                let mut write_pos = 20;
                for p in (0..PIXELS_PER_PACKET).step_by(2) {
                    let pixel_x = h_offset as usize + p;
                    let bar_idx = (pixel_x / (WIDTH / 8)).min(7);
                    let (y, cb, cr) = COLOR_BARS[bar_idx];
                    let y0 = y;
                    let y1 = y;
                    // Pack 40 bits across 5 bytes
                    packet_buffer[write_pos] = (cb >> 2) as u8;
                    packet_buffer[write_pos + 1] = (((cb & 0x03) << 6) | (y0 >> 4)) as u8;
                    packet_buffer[write_pos + 2] = (((y0 & 0x0F) << 4) | (cr >> 6)) as u8;
                    packet_buffer[write_pos + 3] = (((cr & 0x3F) << 2) | (y1 >> 8)) as u8;
                    packet_buffer[write_pos + 4] = (y1 & 0xFF) as u8;
                    write_pos += 5;
                }
                socket.send_to(&packet_buffer[..write_pos], target_addr)?;
                sequence_number = sequence_number.wrapping_add(1);
                h_offset += PIXELS_PER_PACKET as u16;
            }
        }
        println!("Transmitted frame {}", frame_idx + 1);
        timestamp = timestamp.wrapping_add(1500); // Advance 90 kHz RTP clock for 60 fps
        // Sleep only for remaining time in the 16.6ms window
        let elapsed = frame_start.elapsed();
        if elapsed < frame_duration {
            thread::sleep(frame_duration - elapsed);
        }
    }
    println!("Transmission finished successfully");
    Ok(())
}