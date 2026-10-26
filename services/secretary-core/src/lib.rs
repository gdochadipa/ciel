//! Composition exports. Domain/application never import HTTP or concrete adapters.
pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;
pub use application::bus::CommandBus;
pub use domain::{error::CoreError, registry::Registry};
pub use infrastructure::google::GoogleClient;
pub use presentation::http::{router, AppState};
// Compatibility paths for existing adapter clients; implementations live in their layers.
pub use application::writes;
pub use domain::{projection, registry, write_model};
pub use infrastructure::{google, journal};
