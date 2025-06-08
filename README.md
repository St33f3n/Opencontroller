# OpenController

**A gamepad-controlled interface for Smart Home and Maker applications built with modern Rust**

![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)
![License](https://img.shields.io/badge/license-MIT-blue.svg?style=for-the-badge)
![Status](https://img.shields.io/badge/status-In%20Development-yellow.svg?style=for-the-badge)

## What is OpenController?

OpenController started as an exploration into building a unified control interface using a gamepad for various maker and Smart Home protocols. The core idea: why not debug your MQTT infrastructure, control RC vehicles, and interact with different wireless protocols all through the familiar interface of a game controller?

Currently, it's a working proof-of-concept that demonstrates some pretty neat Rust patterns and showcases what's possible when you combine tokio's async runtime with egui's immediate-mode GUI paradigm.

## Current Features

### Multi-Protocol Support
- **MQTT Integration**: Connect to brokers, subscribe to topics, and send messages - great for debugging IoT setups
- **ELRS Control**: Basic support for ExpressLRS RC protocol (early stage)
- **Keyboard Mapping**: Transform controller input into keyboard events with customizable mappings

### Architecture Highlights
- **8 Specialized Threads**: Demonstrates clean separation of concerns in async Rust
- **State Machine Pattern**: Uses the `statum` crate for compile-time guaranteed state transitions
- **Channel-based Communication**: Showcases different tokio synchronization primitives (mpsc, watch, RwLock)
- **Session Management**: Persistent configuration with automatic saving and loading

### UI and UX
- **Immediate-Mode GUI**: Built with egui for responsive, game-like interfaces
- **Dual Input**: Both gamepad and touch/mouse input supported
- **Modular Design**: Clean separation between different protocol handlers

## Technology Stack

This project is essentially a playground for modern Rust async patterns:

```rust
// Core technologies
tokio = "1.43.1"           // Async runtime with excellent channel primitives
eframe = "0.31.1"          // egui integration for immediate-mode GUI
statum = "0.1.48"          // State machines with compile-time guarantees
gilrs = "0.11.0"           // Cross-platform gamepad input
rumqttc = "0.24.0"         // Pure Rust MQTT client
crsf = "2.0.1"             // ELRS/Crossfire protocol implementation
```


## Installation & Setup

### Prerequisites
- Rust 1.70+ (uses modern async/await patterns)
- A gamepad (Xbox controllers work great via xpad protocol)
- Linux preferred (developed primarily on Raspberry Pi)

### Building
```bash
git clone https://github.com/yourusername/opencontroller.git
cd opencontroller

# Development build with full logging
RUST_LOG=info cargo run

# Release build for performance testing
cargo build --release && ./target/release/opencontroller
```

## Interesting Implementation Details

### Thread Architecture
The application runs 8 specialized threads that demonstrate different async patterns:

1. **Controller Collection** - Raw gamepad input using `gilrs`
2. **Controller Processing** - Event validation and state machine transitions
3. **Mapping Engines** (2x) - Parallel processing for different output protocols
4. **MQTT Handler** - State machine for connection management
5. **UI Thread** - egui immediate-mode rendering
6. **Persistence Worker** - Configuration management with oneshot channels
7. **Auto-save Worker** - Background safety net for configuration

### State Machine Integration
Uses `statum` for compile-time guaranteed state transitions:

```rust
#[state]
enum MappingEngineState {
    Initializing,
    Configured, 
    Active,
    Deactivating,
}

// Transitions are validated at compile time
let engine = engine.initialize()?.configure(strategy)?.activate();
```

### Channel Architecture
Demonstrates different tokio synchronization primitives based on use case:
- `mpsc` for event streams (n:1 communication)
- `watch` for state updates (1:n broadcasting)  
- `RwLock` for shared configuration access
- `oneshot` for request/response patterns

## Current Limitations

This is very much a work-in-progress exploration:

- ELRS integration is basic (proof-of-concept level)
- Error handling varies between modules (still being standardized)
- Some features are mockups in the UI
- Performance optimization is ongoing
- Documentation needs cleanup

## Future Directions

The goal is to expand this into a more comprehensive tool:

- Additional wireless protocols (433MHz, LoRa, etc.)
- Plugin system for extensibility
- Better hardware abstraction
- More sophisticated mapping engines
- Improved error handling and recovery

## Contributing

This project is great for learning modern Rust patterns! Areas where contributions would be valuable:

- **Protocol implementations**: Adding new wireless standards
- **UI improvements**: egui is very flexible for experimentation
- **Performance optimization**: Especially for resource-constrained devices
- **Testing**: Cross-platform validation and edge case handling
- **Documentation**: Examples and tutorials for the patterns used

## Development

```bash
# Run with debug logging
RUST_LOG=debug cargo run

# Check formatting and linting
cargo fmt && cargo clippy

# Run tests
cargo test
```

## License

MIT License - see [LICENSE](LICENSE) for details.

---

**Built with enthusiasm for Rust's async ecosystem and immediate-mode GUIs!**

If you're interested in async Rust patterns, state machines, or building responsive UIs, this project might be worth exploring. The codebase demonstrates several interesting patterns that could be useful in other projects.
