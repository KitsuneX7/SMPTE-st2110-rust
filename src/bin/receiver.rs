use smpte_streaming::{Frame, Payload, RTP};
use std::net::UdpSocket;

fn main() -> std::io::Result<()> {
    // First line binds to local host only, second binds to all network interfaces
    let socket = UdpSocket::bind("127.0.0.1:5004")?;
    // let socket = UdpSocket::bind("0.0.0.0:5004")?;
    let mut buffer = [0u8; 9216];
    // Setting the frame buffer
    let mut frame = Frame::new();
    let mut first_packet = true;
    let mut last_sequence_number = 0u16;
    let mut total_frames_rendered = 0usize;
    let mut total_packets_dropped = 0usize;
    loop {
        // Receiving a packet
        let (bytes_received, _src_addr) = socket.recv_from(&mut buffer)?;
        // Packets under 20 bytes are guaranteed to be corrupted
        if bytes_received < 20 {
            continue;
        }
        // Parse RTP header
        let rtp = RTP::parse(&buffer[0..12]);
        // Deal with invalid RTP packets
        if !rtp.valid_version() {
            continue;
        }
        // Deal with packet loss / duplication
        if first_packet {
            last_sequence_number = rtp.sequence_number();
            first_packet = false;
        } else {
            let diff = rtp.sequence_number().wrapping_sub(last_sequence_number) as i16;
            if diff > 1 {
                // Dropped (diff - 1) packets
                total_packets_dropped += (diff - 1) as usize;
            } else if diff < 1 {
                // Duplicate or out-of-order packet (diff <= 0)
                continue;
            }
            last_sequence_number = rtp.sequence_number();
        }
        // Checking if frame needs to be reset
        if rtp.timestamp() != frame.timestamp() {
            frame.reset(rtp.timestamp());
        }
        // Parse payload
        let payload = Payload::parse(&buffer[12..bytes_received]);
        for index in 0..payload.count() {
            let srd = payload.srd(index);
            let ln = srd.line_number();
            let ho = srd.horizontal_offset();
            let ps = payload.pixel_slice(&buffer, index);
            frame.write_pixel_slice(ln, ho, ps);
        }
        // Record the packet that was successfully parsed
        frame.record_packet();
        if rtp.marker() {
            total_frames_rendered += 1;
            println!("Successfully parsed frame {total_frames_rendered}");
        }
        // Exit loop after capturing target frames so disk writing does not block ingestion
        if total_frames_rendered == 5 {
            break;
        }
    }
    // Save assembled frame to disk after exiting the hot receive loop
    println!("Saving frame_01.ppm to disk...");
    frame.save_ppm("frame_01.ppm")?;
    println!("Successfully terminated with a total of {total_packets_dropped} packets dropped");
    Ok(())
}