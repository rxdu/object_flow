//! ObjectFlow's engine (ADR-0132): flows declared in the flow format, their
//! rules enforced on every request, and everything they produce recorded.
//!
//! The core is synchronous. Its first slice (ADR-0153) runs a request on one
//! object, in the order of refusals of `DESIGN.md` §6, over a SQLite store laid
//! out as `docs/design/storage-schema.md` says; a request that reaches a
//! construct beyond the slice is answered as not run, naming it.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod value;

pub mod expr;

pub mod yaml;

pub mod model;

pub mod eval;
pub mod world;

pub mod verdict;
pub mod wire;

pub mod store;

pub mod request;

pub mod reads;
