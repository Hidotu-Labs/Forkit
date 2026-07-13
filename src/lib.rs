// Library surface for integration tests and benchmarks.
// The binary entry point is src/main.rs.

pub mod html5;
pub mod css;
pub mod render;

// Re-export net so benchmarks/tests can resolve URLs if needed.
pub mod net;
pub mod js;
pub mod ui;
pub mod app;
pub mod window;
