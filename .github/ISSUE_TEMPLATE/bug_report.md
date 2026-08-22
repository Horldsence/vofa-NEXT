---
name: Bug report
about: Report a reproducible problem in vofa-NEXT
title: '[Bug] '
labels: bug
assignees: ''

---

**Describe the bug**
A clear and concise description of what the bug is.

**To Reproduce**
Steps to reproduce the behavior:
1. Connect to '...' (serial / TCP / UDP / ...)
2. Configure '...' (baud rate, protocol, widget, ...)
3. Send / receive '...'
4. See error

**Expected behavior**
A clear and concise description of what you expected to happen.

**Actual behavior / error output**
What actually happened. Paste any error messages, panics, or console output here (wrap in a code block).

```
paste logs here
```

**Screenshots / recordings**
If applicable, add screenshots or a screen recording to help explain your problem.

**Environment (please complete the following information):**
- vofa-NEXT version or commit: [e.g. 0.1.0, or `git rev-parse HEAD`]
- OS and version: [e.g. macOS 15.2, Windows 11 23H2, Ubuntu 24.04]
- Connection type: [e.g. serial @ 115200, TCP client, UDP]
- Connected device / firmware (if relevant): [e.g. STM32 running FireWater protocol]
- Rust / Node toolchain (only if building from source): [e.g. rustc 1.85, node 20]

**Data sample (if applicable)**
A minimal hex/text sample of the data stream that triggers the problem. Remove any sensitive payload first.

**Will you submit a PR to fix this?**
Yes / No / Need guidance.

> Note: AI-assisted ("vibe") PRs are welcome, but they must meet the hard requirements in [`docs/AGENTS.md`](/docs/AGENTS.md) — all checks (`pnpm typecheck`, `pnpm test`, `cargo test --workspace`, `cargo clippy --workspace --all-targets` with zero new warnings) must actually be run and pass, the fix must come with a test, and the code must not contain session traces (Stage/Phase/Task markers). Unverified PRs will be asked to rework.

**Additional context**
Add any other context about the problem here: does it happen every time, only with certain data rates, only after reconnecting, etc.
