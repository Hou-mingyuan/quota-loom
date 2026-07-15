pub mod core;

#[cfg(feature = "desktop")]
mod desktop;

#[cfg(feature = "desktop")]
pub fn run() {
    desktop::run();
}
