# Load test

`scripts/loadtest.sh <N> [duration]` runs `lk load-test` against a LiveKit server: one
room with N participants, of which min(N, 10) publish simulcast video (medium, cameras
on) and the rest subscribe with a 3x3 grid layout, like the TurtleMeet UI.

## Results: 2026-11-03, single laptop

Machine: Apple M1, 8 cores, 8 GB RAM, macOS. `livekit-server --dev` and the `lk` load
generator ran **on the same machine** over loopback, 45 s per run. Server CPU is for the
livekit-server process (100% = one core).

| N | Cameras / viewers | Video tracks received | Avg bitrate per viewer | Packet loss | Server CPU avg / peak | Server RSS |
|---|---|---|---|---|---|---|
| 10 | 10 / 0 | (no viewers in this split) | - | - | 4% / 39% | 164 MB |
| 25 | 10 / 15 | 135/150 | 4.7 Mbps | 0% | 36% / 49% | 148 MB |
| 50 | 10 / 40 | 360/400 | 4.7 Mbps | 0% | 79% / 128% | 263 MB |
| 100 | 10 / 90 | 810/900 | 0.56 Mbps | **31%** | 148% / 193% | 461 MB |

"9 of 10" tracks per viewer is expected: the 3x3 layout shows 9 tiles.

## What this does and doesn't show

- Up to **50 participants** (10 cameras) the SFU delivered every expected track with
  0% loss, using about 1 core.
- At **100** delivery degraded badly (31% loss, bitrate collapsed). The server used under
  2 of 8 cores, so the server CPU was not the limit. The likely bottleneck is the load
  generator and macOS loopback UDP buffers sharing the same laptop. **This run cannot
  say whether TurtleMeet handles 100.**
- Not measured: end-to-end latency, real browsers, the Rust API under load (join and
  token minting), all cameras on, and audio.

**No capacity claim is made from these numbers.** To get one: run the server on the
target VPS (`deploy/`) and `lk load-test` from one or more separate machines, then repeat
10/25/50/100 and record server CPU, memory and bandwidth (`docker stats`, `ifstat`).
