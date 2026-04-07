//! Digital twin of the GitHub Releases API.
//! A stateful, in-process mock server that models the real API behavior:
//! - Draft/published release state machine
//! - Asset upload with duplicate rejection
//! - Repository dispatch events
//! - Rate limit headers
//!
//! Uses std::net::TcpListener — zero dependencies.

pub mod crates;
pub mod fault;
pub mod github;
pub mod npm;
pub mod pypi;

pub use crates::CratesTwin;
pub use fault::FaultProxy;
pub use github::GitHubTwin;
pub use npm::NpmTwin;
pub use pypi::PyPITwin;
