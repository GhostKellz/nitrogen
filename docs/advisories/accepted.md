# Accepted Advisories

Audit performed: 2026-06-17

`cargo audit` currently reports one accepted warning and no known vulnerability failures for the checked lockfile.

| Advisory | Crate | Severity | Source | Disposition |
|----------|-------|----------|--------|-------------|
| RUSTSEC-2025-0141 | `bincode 1.3.3` | Warning, unmaintained | `webrtc-dtls` via `webrtc 0.11` | Accepted until upstream WebRTC stack removes `bincode 1.x` |

## Dependency Update Notes

Nitrogen now resolves the full PipeWire stack to the Rust `0.10` crates. This avoids mixing `libspa-sys` versions and fixes build failures against the current `ghoststream` dependency graph.
