# SMPTE ST 2110-20 Specification & High-Performance Rust Implementation

## 1. Industry Context & The EVS Target

In professional broadcast production, uncompressed video transport has migrated from legacy coax cables (**SDI / 3G-SDI**) to IP networks via the **SMPTE ST 2110** standard suite. 

**EVS** (famed for their XT-VIA live production servers, LSM-VIA replay remotes, and Neuron IP processing platforms) operates almost entirely on ST 2110 and ST 2022 infrastructure. 

Unlike compressed consumer streaming (H.264/H.265/AV1 over RTMP or HLS), ST 2110 splits essence streams into separate multicast flows synchronized via PTP (SMPTE ST 2059-2 / IEEE 1588):
* **ST 2110-20**: Uncompressed Active Video
* **ST 2110-30**: Uncompressed PCM Audio (AES67)
* **ST 2110-40**: Ancillary Data (Captions, Timecode, VANC)

For a tactical job-hunting demo, implementing a **SMPTE ST 2110-20 packet receiver and frame reassembler in Rust** demonstrates deep mastery of networking, memory layout, SIMD, and real-time broadcast systems.

---

## 2. Target Format Profile: Full HD @ 60 Hz

For broadcast Full HD at $60\text{ fps}$ (specifically $1080\text{p}59.94$ or $1080\text{p}60$), the standard broadcast raster is:

* **Active Resolution:** $1920 \times 1080$ active pixels.
* **Scan Type:** Progressive (`1080p`).
* **Frame Rate:** $60\text{ fps}$ (or $59.94\text{ fps}$, interval $\approx 16.67\text{ ms}$).
* **Color Space / Sampling:** $YC_bC_r$ 4:2:2 (Industry broadcast standard).
* **Sample Bit Depth:** 10-bit per component ($10\text{ bpc}$).

### Bandwidth Calculation

In $YC_bC_r$ 4:2:2, each pixel requires 1 luma sample ($Y$) and an average of 1 chroma sample ($C_b$ or $C_r$):
$$\text{Average Bits Per Pixel} = 2 \text{ components} \times 10 \text{ bits} = 20 \text{ bits/pixel}$$

Total raw video bitrate:
$$\text{Bitrate} = 1920 \times 1080 \times 60 \times 20 \approx 2,488,320,000 \text{ bps} \approx 2.488 \text{ Gbps}$$

Because this exceeds 1 Gbps, 1080p60 ST 2110 flows operate over **10 GbE, 25 GbE, or 100 GbE** network interfaces.

---

## 3. Network Encapsulation Stack

```
+-------------------------------------------------------+
| Ethernet II Header (14 bytes)                         |
+-------------------------------------------------------+
| IPv4 Header (20 bytes)                                |
+-------------------------------------------------------+
| UDP Header (8 bytes)                                  |
+-------------------------------------------------------+
| RTP Header (RFC 3550) (12 bytes)                      |  <-- Frame boundary marker (M=1)
+-------------------------------------------------------+
| ST 2110-20 Payload Header (RFC 4175 SRD) (4+ bytes)   |  <-- Line number & line offset
+-------------------------------------------------------+
| Packed Pixel Groups (pgroups) (~1200 - 1400 bytes)    |  <-- 10-bit 4:2:2 samples
+-------------------------------------------------------+
```

---

## 4. Header Anatomy & The "Byte with a 1"

### 4.1. RTP Header (RFC 3550, 12 Bytes)

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

#### The Frame Boundary Indicator: The Marker Bit ($M$)
In Byte 1:
$$\text{Byte 1} = (M \ll 7) \mid (PT \ \& \ 0\text{x}7F)$$
* **$M$ (Marker Bit, bit 7 of byte 1):** In ST 2110-20, **$M = 1$ specifically on the very last packet of a video frame**. On every other packet of that frame, $M = 0$.
* **$PT$ (Payload Type, 7 bits):** Dynamic payload type (typically in the range $96 \le PT \le 127$).
* **Sequence Number (16 bits):** Increments by 1 for each RTP packet. Used to detect packet loss and packet reordering.
* **Timestamp (32 bits):** Derived from a 90 kHz clock referenced to the PTP grandmaster. All packets comprising the same video frame share the exact same timestamp.

---

### 4.2. ST 2110-20 Payload Header (RFC 4175 SRD - Sample Row Data)

Directly following the 12-byte RTP header is the Sample Row Data (SRD) header:

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    Extended Sequence Number   |          Length (bytes)       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|F|     Line Number (15 bits)   |C|    Offset in Line (15 bits) |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

* **Extended Sequence Number (16 bits):** Extends the 16-bit RTP sequence number to 32 bits to prevent rollover aliasing at 10 Gbps speeds.
* **Length (16 bits):** Byte length of the pixel payload following this SRD header.
* **$F$ (Field ID, 1 bit):** Set to `0` for progressive frames (1080p) or Field 1; set to `1` for Field 2 in interlaced video.
* **Line Number (15 bits):** The vertical scan line index ($0 \le \text{Line} < 1080$).
* **$C$ (Continuation bit, 1 bit):** If set to `1`, another SRD header follows immediately after the pixel payload in the same UDP packet (used when a packet carries samples spanning across line boundaries).
* **Offset in Line (15 bits):** The pixel offset from the start of the raster line ($0 \le \text{Offset} < 1920$).

---

## 5. Pixel Grouping: 10-bit $YC_bC_r$ 4:2:2 (`pgroup`)

In 10-bit 4:2:2, samples do not land on 8-bit byte boundaries. ST 2110-20 packs pairs of pixels into a **5-byte pixel group (`pgroup`)** spanning 40 bits:

$$\text{1 pgroup} = 2 \text{ pixels} = (C_{b0}, Y_0, C_{r0}, Y_1) = 4 \times 10 \text{ bits} = 40 \text{ bits} = 5 \text{ bytes}$$

### Bit Layout Across 5 Bytes:

```
Byte 0:  [ Cb0[9:2]                                    ]
Byte 1:  [ Cb0[1:0] | Y0[9:4]                          ]
Byte 2:  [ Y0[3:0]  | Cr0[9:6]                         ]
Byte 3:  [ Cr0[5:0] | Y1[9:8]                          ]
Byte 4:  [ Y1[7:0]                                     ]
```

To convert a pixel offset to a byte offset on a line:
$$\text{Byte Offset} = \left(\frac{\text{Pixel Offset}}{2}\right) \times 5$$

Each 1920-pixel line consists of:
$$\frac{1920}{2} = 960 \text{ pgroups} = 960 \times 5 = 4800 \text{ bytes}$$

A complete $1920 \times 1080$ frame buffer is:
$$1080 \times 4800 = 5,184,000 \text{ bytes} \approx 5.184 \text{ MB}$$

---

## 6. Rust Project Architecture: The Assembler Engine

To stand out to EVS engineering leads, the Rust design must prioritize zero allocations on the hot path, memory safety, and high-throughput cache locality.

```
+--------------------------------------------------------------------------+
|                          Network Ingress Layer                           |
|       (UdpSocket with recvmmsg / socket2 / io_uring / AF_XDP)            |
+--------------------------------------------------------------------------+
                                     |
                                     v
+--------------------------------------------------------------------------+
|                        RtpSt2110Parser (Zero-Copy)                       |
|   - Parse RTP Header (Check M bit)                                       |
|   - Parse SRD Header (Line number, pixel offset)                         |
|   - Validate sequence numbers & timestamps                               |
+--------------------------------------------------------------------------+
                                     |
                                     v
+--------------------------------------------------------------------------+
|                           FrameSlotRingBuffer                            |
|   - Preallocated fixed buffer pool (e.g., 4 x 5.184 MB slots)            |
|   - Direct scatter-write: Copy packet payload into calculated offset:    |
|       dest_ptr = slot_base + (line * 4800) + (offset / 2 * 5)            |
|   - Track missing packets per frame via bitmask                          |
+--------------------------------------------------------------------------+
                                     |
                                     | When M == 1 and all packets arrived
                                     v
+--------------------------------------------------------------------------+
|                           Frame Dispatcher                               |
|   - Signal frame completion (e.g., crossbeam channel or atomic ring)     |
|   - Pass completed FrameSlot to consumer (display, recorder, SDI output) |
+--------------------------------------------------------------------------+
```

### Essential Rust Data Structures

```rust
/// Parsed RTP Header fields
pub struct RtpHeader {
    pub marker: bool,             // The "1 byte/bit" frame boundary signal
    pub payload_type: u8,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub ssrc: u32,
}

/// Sample Row Data (RFC 4175)
pub struct SampleRowData {
    pub extended_seq_num: u16,
    pub length: u16,
    pub field_id: u8,
    pub line_number: u16,
    pub continuation: bool,
    pub offset: u16,
}
```

---

## 7. Strategic Project Roadmap for EVS

| Milestone | Objective | Key Low-Level Skills Demonstrated |
| :--- | :--- | :--- |
| **Stage 1: Packet Wire Parser** | Parse RTP + ST 2110-20 SRD headers with zero allocations using byte slices (`&[u8]`). | Pointer arithmetic, endianness conversion (`u16::from_be_bytes`), bit manipulation. |
| **Stage 2: Frame Buffer Reassembler** | Assemble full $5.184\text{ MB}$ frames across packet boundaries using the marker bit ($M=1$) and timestamp tracking. | Memory layout, preallocated slab allocator, circular ring buffers. |
| **Stage 3: Jitter & Packet Loss Handling** | Handle out-of-order packets via 32-bit extended sequence numbers and drop corrupted frames. | Real-time state machine, latency budgeting. |
| **Stage 4: High-Performance Ingress** | Upgrade from standard `std::net::UdpSocket` to `recvmmsg` (via `nix` or `socket2`), or `io_uring`. | Linux kernel bypass concepts, batch syscalls. |
| **Stage 5: SIMD Unpacker (Bonus)** | Unpack 10-bit $YC_bC_r$ `pgroup` to 8-bit or 16-bit RGB/YUV using AVX2 or NEON intrinsics. | Data-parallel optimization, zero-copy video pipelines. |