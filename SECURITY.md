# Security Policy

## Reporting Vulnerabilities

Please report security issues privately.

1. Do not open a public GitHub issue for vulnerabilities.
2. Email the maintainers listed in [CONTRIBUTING.md](CONTRIBUTING.md), or use GitHub private vulnerability reporting if it is enabled for the repository.
3. Include affected versions, platform details, reproduction steps, logs, and whether the issue requires local access.
4. Allow reasonable time for a fix before public disclosure.

## Supported Versions

Nitrogen is pre-1.0 and under active development.

| Version | Supported |
|---------|-----------|
| 0.2.x | Yes |
| < 0.2 | No |

## Security Considerations

### Local Capture

Nitrogen captures screen, window, desktop audio, and microphone streams through PipeWire and xdg-desktop-portal. Portal prompts should be treated as the security boundary for capture consent.

### Device Access

Some features require access to local devices or system services.

| Feature | Access | Notes |
|---------|--------|-------|
| Screen/window capture | xdg-desktop-portal + PipeWire | User consent through portal picker |
| Desktop/microphone audio | PipeWire | Captures selected audio sources |
| NVENC encoding | NVIDIA driver libraries | Local GPU encoding only |
| Global hotkeys | `/dev/input/event*` read access | Usually requires `input` group membership |
| Virtual camera | PipeWire | Exposes `Nitrogen Camera` to local apps |

### Network Outputs

RTMP, RTMPS, SRT, and WebRTC outputs can send captured content over the network. Treat stream URLs, keys, and browser-accessible WebRTC endpoints as sensitive.

## Dependency Auditing

Use `cargo audit` before releases and after dependency updates.

```bash
cargo audit
```

Advisory tracking lives in [docs/advisories/](docs/advisories/).

## User Checklist

- Review portal prompts before granting capture access.
- Keep NVIDIA drivers, PipeWire, and xdg-desktop-portal updated.
- Do not share stream keys, WebRTC endpoints, or recordings unintentionally.
- Review logs and support bundles before attaching them to public issues.
- Use global hotkeys only when you are comfortable granting input device read access.
