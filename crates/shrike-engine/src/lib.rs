#![forbid(unsafe_code)]
//! Command dispatch shared by the binary and tests.

mod analyze;
mod cache;
mod dispatch;
mod payloads;

pub use crate::dispatch::{DispatchResult, dispatch, write_response};
