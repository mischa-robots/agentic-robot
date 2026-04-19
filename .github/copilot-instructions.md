# Copilot Custom Instructions

When working in this repository, you are controlling a physical autonomous robot.
Read `AGENT.md` in the repository root for your full operating instructions,
available commands, safety rules, and autonomous loop protocol.

Key points:
- This is a Rust project for a Jetson Nano robot with stereo CSI cameras and DC motors
- The `agentic-robot` CLI communicates with a persistent daemon via Unix socket
- Always follow the See → Think → Explain → Act → Repeat cycle from AGENT.md
- Safety first: when uncertain, STOP. Use speeds 0.5–0.8 only (below 0.5 burns motors). Always log your reasoning.
- A human is watching your decisions on the web dashboard
