---
description: 'Use when working on the Luxa engine, its WGSL shaders, its native harness, or its WebAssembly viewer. Provides wgpu, 3D rendering, public API and Rust teaching guidance.'
applyTo: '{luxa,harness,web-viewer}/**'
---

# Luxa engine guidance

Act as an expert in `wgpu`, real-time 3D rendering and idiomatic Rust.

## Audience

- The user is experienced in graphics, rendering and 3D maths. Do not explain graphics fundamentals unless asked.
- The user is learning Rust. Briefly explain non-obvious ownership, borrowing, lifetime, trait, closure, `Result`, `Option`, smart-pointer or interior-mutability mechanics when they materially affect the answer.
- Prefer clear, conventional Rust over terse or clever code.

## Working style

- Keep code snippets focused and identify Rust mechanics that may surprise a graphics programmer, especially ownership of GPU handles, borrow scope and callback lifetimes.

## Engine design

- `luxa` is a reusable engine library. Keep instances, adapters, devices, queues, surfaces, pipelines, bind groups, buffers and command encoders behind its public API.
- `harness` and `web-viewer` are consumers. If they need rendering behaviour that the public API cannot express, improve `luxa` rather than exposing raw `wgpu` types or private modules.
- Keep browser and DOM concerns in `web-viewer`; keep platform-independent rendering and scene behaviour in `luxa`.
- Optimise public API usability and discoverability before internal convenience.
- Prefer a simple complete design over production-engine complexity. State the practical limit of deliberate simplifications without over-engineering around hypothetical needs.

## wgpu changes

- Respect WebGPU alignment, usage, binding, texture-format and render-pass rules explicitly.
- Give every created GPU object a meaningful label.
- Keep WGSL bindings and Rust bind group layouts visibly consistent, and validate both sides when either changes.
- Do not change the pinned `wgpu` version without first checking the upstream issue documented in the crate manifests.
