mod app;
mod assets;
mod bootstrap;
mod notes;
mod settings;
mod sync;

pub use app::AppCore;
pub use bootstrap::{BootstrapState, SyncAccountState};

#[cfg(test)]
mod tests;
