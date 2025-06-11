//! Controller subsystem for gamepad input handling
//!
//! Implements a two-stage processing pipeline:
//!
//! 1. [`event_collector`] - Raw gamepad input collection
//! 2. [`event_processor`] - Event transformation and filtering
//! 3. [`controller_handle`] - Unified API and lifecycle management
//!
//! # Architecture
//!
//! ```text
//! Gamepad ──► Collector ──► Processor ──► ControllerOutput
//!             (Raw Events)  (Filtered)
//! ```
//!
//! The subsystem runs in separate threads with 130ms processing intervals
//! optimized for human reaction times.

pub mod controller_handle;
pub mod event_collector;
pub mod event_processor;
