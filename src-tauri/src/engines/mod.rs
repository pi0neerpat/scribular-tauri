//! Test doubles for the native engines (KTD9), compiled in with the
//! `fake-engines` feature.

#[cfg(any(test, feature = "fake-engines"))]
pub mod fake;
