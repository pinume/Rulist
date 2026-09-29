#[cfg(not(target_os = "linux"))]
compile_error!("Rulist only supports Linux.");

pub mod app;
pub mod auth;
pub mod config;
pub mod db;
pub mod filesystem;
pub mod interactive;
pub mod permissions;
pub mod preview;
pub mod server;
mod sign;
mod static_files;

pub use app::run;
