# SMPTE ST 2110-20 Real-Time Video Ingestion & Transmission Engine

A high-throughput, zero-dependency SMPTE ST 2110-20 (uncompressed active video over IP) ingestion receiver and frame-paced transmitter written in pure Rust using `std` only.

Engineered for ultra-low latency broadcast workflows, this engine parses RTP (RFC 3550) headers and RFC 4175 payloads, unpacks 10-bit 4:2:2 packed pixel groups (`pgroups`), tracks sequence continuity with wrapping modular arithmetic, and rasterizes uncompressed 1080p60 video streams into a pre-allocated canvas.

---

## Architecture Overview

```
+-----------------------------------------------------------------------------+
|                               NETWORK FABRIC                                |
|           10-bit 4:2:2 Uncompressed Video Stream (1080p60 @ ~1.5 Gbps)      |
+-----------------------------------------------------------------------------+
                                       │
                     [ UDP Datagrams: 1220 - 1400 bytes ]
                                       ▼
+-----------------------------------------------------------------------------+
|                              RECEIVER HOT LOOP                              |
|                            (src/bin/receiver.rs)                            |
|                                                                             |
|  1. Ingestion: Stack scratchpad [u8; 9216] via socket.recv_from()           |
|                                                                             |
|  2. RTP Validation (src/rtp.rs):                                            |
|     - Header verification (V=2, dynamic PT=96)                              |
|     - RFC 3550 wrapping modular sequence tracking (loss / reorder check)    |
|                                                                             |
|  3. Payload Deconstruction (src/payload.rs & src/srd.rs):                   |
|     - Stack-allocated [SRD; 3] header parsing (ESN + Continuation chain)    |
|     - Line number and horizontal pixel offset bitfield extraction           |
|                                                                             |
|  4. Zero-Copy Scanline Blitting (src/frame.rs):                             |
|     - Direct memory blit into pre-allocated Box<[u8]> (5.18 MB canvas)     |
|     - Coordinate mapping: (line * 4800) + (offset * 5 >> 1)                 |
|                                                                             |
|  5. Frame Demarcation & Export:                                             |
|     - RTP Marker bit detection (end-of-frame boundary)                      |
|     - Conversion: 10-bit YCbCr -> 8-bit RGB Netpbm (.ppm) rasterization     |
+-----------------------------------------------------------------------------+
```

---

## Project Structure

The project isolates core protocol parsers into reusable library modules and separates binary entry points inside `src/bin/`:

```text
.
├── Cargo.toml
└── src/
    ├── lib.rs              # Module exports (rtp, srd, payload, frame)
    ├── rtp.rs              # RFC 3550 RTP transport header parser & sequence ordering
    ├── srd.rs              # RFC 4175 Scanline Raw Data (SRD) bitfield extraction
    ├── payload.rs          # RFC 4175 payload parsing & pixel slice boundary mapping
    ├── frame.rs            # 5.18 MB frame canvas (Box<[u8]>) & Netpbm PPM exporter
    └── bin/
        ├── receiver.rs     # Hot-loop UDP ingestion, loss tracking, and frame assembly
        └── sender.rs       # 10-bit SMPTE test bar generator with 60 fps deadline pacing
```

---

## Wire Protocols & Bitfield Memory Layouts

### 1. RTP Transport Header (RFC 3550)

Every incoming packet begins with a 12-byte fixed transport header:

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|V=2|P|X|  CC   |M|     PT      |       Sequence Number         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                           Timestamp                           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|           Synchronization Source (SSRC) identifier            |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

* **Version (`V=2`)**: Validated via `rtp.valid_version()` (`packet[0] >> 6 == 0b10`).
* **Marker (`M`)**: Bit 7 of byte 1. Set to `1` on the final packet of a video frame to trigger frame completion and statistics updates.
* **Sequence Number**: 16-bit monotonic counter. Gaps and packet reordering are tracked using signed wrapping difference arithmetic.
* **Timestamp**: 90 kHz presentation clock shared by all packets comprising the same raster frame.

---

### 2. RFC 4175 Payload & SRD Headers

Following the 12-byte RTP header, the payload contains a 2-byte Extended Sequence Number (ESN) followed by one to three 6-byte Scanline Raw Data (SRD) headers:

```
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|   Extended Sequence Number    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|          Length (u16)         |F|       Line Number (15)      |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|C|    Horizontal Offset (15)   |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

* **Length (`u16`)**: Exact byte count of pixel data following the SRD headers for this chunk.
* **Field (`F`)**: Interlacing bit (bit 15 of word 1; `0` for progressive 1080p).
* **Line Number (`0..1079`)**: Vertical raster position extracted via `field_line_number & 0x7FFF`.
* **Continuation (`C`)**: Bit 15 of word 2. When set to `1`, another 6-byte SRD header immediately follows.
* **Horizontal Offset (`0..1919`)**: Horizontal pixel column index extracted via `continuation_horizontal_offset & 0x7FFF`.

---

### 3. SMPTE ST 2110-20 10-Bit 4:2:2 `pgroup` Layout

In 10-bit 4:2:2 sampling, 2 adjacent horizontal pixels share chroma ($C_b, C_r$) while retaining individual luma values ($Y_0, Y_1$). Four 10-bit samples ($40\text{ bits}$) are packed across 5 octets:

```
Byte 0: [ Cb[9:2]                                          ]
Byte 1: [ Cb[1:0]  | Y0[9:4]                               ]
Byte 2: [ Y0[3:0]            | Cr[9:6]                     ]
Byte 3: [ Cr[5:0]                      | Y1[9:8]           ]
Byte 4: [ Y1[7:0]                                          ]
```

* **Scanline Byte Width**: $1920\text{ pixels} \times 2.5\text{ bytes/pixel} = 4800\text{ bytes per line}$.
* **Frame Canvas Footprint**: $1080\text{ lines} \times 4800\text{ bytes} = 5,184,000\text{ bytes}$ ($\approx 5.18\text{ MB}$).

---

## Architectural & Systems Decisions

### 1. Zero Allocations in the Ingestion Path

At 1080p60, an uncompressed video stream pushes **$\sim 4,320\text{ UDP packets per frame}$** ($\sim 260,000\text{ packets/second}$). Dynamic heap allocations in the hot path trigger cache misses, lock contention, and unpredictable latency spikes.

* **Stack-Allocated Datagram Scratchpad:** Every incoming UDP packet is received directly into a stack buffer (`[u8; 9216]`), sized to accommodate standard MTU up to 9K Jumbo Frames.
* **Single Pre-Allocated Canvas:** The full active frame canvas ($1920 \times 1080 \times 2.5 = 5,184,000\text{ bytes}$) is allocated exactly once at startup as a boxed slice (`Box<[u8]>`).
* **In-Place Frame Overwrites:** New frames update the existing buffer in-place. The buffer is never re-zeroed, as the 4,320 incoming packets sequentially overwrite every byte.

---

### 2. On-the-Fly Bit Shifts vs. Struct Inflating

Instead of expanding raw wire data into large intermediate structs with owned fields (which inflates memory footprint and pollutes CPU cache lines), `RTP`, `SRD`, and `Payload` are compact `Copy` structs that extract values on demand:

```rust
// In src/srd.rs: extracts bitfield on the fly without extra storage
#[inline]
pub fn line_number(&self) -> u16 {
    self.field_line_number & 0x7FFF
}

#[inline]
pub fn horizontal_offset(&self) -> u16 {
    self.continuation_horizontal_offset & 0x7FFF
}
```

This keeps parser metadata minimal and allows the compiler to keep hot data in CPU registers and L1/L2 caches.

---

### 3. Wrapping Modular Sequence Tracking

RTP sequence numbers are 16-bit integers ($0 \dots 65535$) that wrap around every $\sim 15\text{ seconds}$ at 60 fps. Discontinuities and packet ordering are tracked without branch mispredictions using two's complement modular arithmetic:

```rust
let diff = rtp.sequence_number().wrapping_sub(last_sequence_number) as i16;
if diff > 1 {
    // Dropped (diff - 1) packets
    total_packets_dropped += (diff - 1) as usize;
} else if diff < 1 {
    // Duplicate or out-of-order packet (diff <= 0)
    continue;
}
last_sequence_number = rtp.sequence_number();
```

---

### 4. Direct Scanline Blit Math

Incoming pixel slices are placed directly into the canvas using bitwise arithmetic, avoiding integer division:

```rust
let begin = (line as usize) * BYTES_PER_LINE + ((offset as usize) * 5 >> 1);
let end = begin + packet_payload.len();
if end > FRAME_SIZE {
    return;
}
self.buffer[begin..end].copy_from_slice(packet_payload);
```

---

### 5. Frame-Deadline Pacing & ST 2110-21 Traffic Shaping

* **Paced Transmitter (`src/bin/sender.rs`):** Operating system schedulers cannot reliably sleep for microsecond intervals (e.g., $14\,\mu\text{s}$ per line) due to timer quantization. The transmitter measures whole-frame budgets ($16,666\,\mu\text{s}$) using monotonic clocks (`std::time::Instant`), sleeping once per frame for the remaining duration to sustain an exact **60.0 fps** rate.
* **Deferred File I/O:** Serializing a 6.2 MB Netpbm PPM image (`frame.save_ppm()`) takes several milliseconds of blocking disk I/O. Writing during the capture loop causes the OS socket queue to overflow. File export is deferred until after the capture loop finishes.

---

## Verification & Headless Visualization

To verify the rasterizer without external graphics libraries or image decoding crates, the engine serializes directly to **Netpbm P6 (binary RGB24)**.

The unpacker extracts 10-bit samples from each 40-bit `pgroup` and applies standard ITU-R conversion matrices:

$$R = \text{clamp}\left(Y + 1.402 \times (C_r - 128)\right)$$

$$G = \text{clamp}\left(Y - 0.344136 \times (C_b - 128) - 0.714136 \times (C_r - 128)\right)$$

$$B = \text{clamp}\left(Y + 1.772 \times (C_b - 128)\right)$$

---

## Quickstart

### 1. Kernel Buffer Tuning (Linux)

Uncompressed 1080p60 video produces high burst rates. Expand the kernel UDP receive buffer to absorb OS scheduling jitter:

```bash
sudo sysctl -w net.core.rmem_max=26214400
sudo sysctl -w net.core.rmem_default=8388608
```

### 2. Build the Workspace

Compile all binaries with release optimizations:

```bash
cargo build --release
```

### 3. Run the Receiver

In your first terminal, launch the ingestion receiver:

```bash
cargo run --release --bin receiver
```

*Binds to `127.0.0.1:5004`, tracks sequence continuity, captures 5 video frames, and saves `frame_01.ppm`.*

### 4. Run the 10-Bit Test Pattern Transmitter

In a second terminal, launch the frame-paced transmitter:

```bash
cargo run --release --bin sender
```

*Transmits standard 10-bit SMPTE 75% color bars (White, Yellow, Cyan, Green, Magenta, Red, Blue, Black) paced at 60 fps.*

### 5. Inspect Frame Output

Verify the assembled raster image:

```bash
# View using ImageMagick
display frame_01.ppm

# Or convert to PNG
convert frame_01.ppm frame_01.png
```

---

## Technical Specifications

| Parameter | Specification |
| :--- | :--- |
| **Standard** | SMPTE ST 2110-20 / RFC 4175 |
| **Transport Protocol** | RTP over UDP (RFC 3550) |
| **Video Format** | 1080p60 Uncompressed ($1920 \times 1080$) |
| **Color Sampling** | 10-bit 4:2:2 $YC_bC_r$ |
| **pgroup Layout** | 40 bits across 5 octets |
| **Scanline Stride** | 4,800 bytes / line |
| **Frame Canvas Size** | 5,184,000 bytes |
| **RTP Clock Frequency** | 90 kHz |
| **Dependencies** | Pure Rust (`std` only) |

---

## Next Milestones

- [ ] **Lock-Free SPSC Ingestion Queue:** Decouple socket `recv_from` I/O from frame raster blitting using a ring buffer of pre-allocated packet slots.
- [ ] **Kernel Bypass:** Investigate DPDK / Rivermax bindings for zero-copy PCIe DMA directly to host memory.
- [ ] **Compute Shader Offload:** Port 10-bit $YC_bC_r \to RGB$ conversion matrix to a Vulkan compute shader for direct GPU display presentation.