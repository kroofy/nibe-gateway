//! F730 adapter: replay/serial transport, opt-in reads and gated writes.
pub mod catalog;
pub mod decode;
pub mod discovery;
mod runtime;
pub mod transport;
pub mod writes;
pub use runtime::*;
