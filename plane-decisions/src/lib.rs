//! The decisions plane: a request routed by its model to a decision provider, in either of the two

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod claims;
mod codec;
mod config;
mod driven;
mod meta;
mod ops;
mod plane_door;

/// The door: this plugin's one public item.
pub use plane_door::door;

/// The crate's tests that reach past [`door`]: in-crate because the crate's only public item is the
#[cfg(test)]
mod tests {
    mod durable_strings;
}
