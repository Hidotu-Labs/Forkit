// Library surface for integration tests and benchmarks.
// The binary entry point is src/main.rs.

pub mod dom;
pub mod render;

// Re-export net so benchmarks/tests can resolve URLs if needed.
pub mod net;
