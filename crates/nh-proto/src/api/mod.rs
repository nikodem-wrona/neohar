//! The JSON API of the core: envelopes, ids, methods, projections,
//! events and streams, each with a JSON Schema.
//!
//! This module is the contract. `nh api schema` prints one document made from it, and
//! `mise run gen:api` turns that document into the TypeScript types of the UI. Everything is
//! additive within the major version `neohar/1`: a field gets added with a default, a
//! method gets added, and nothing is renamed or removed.
//!
//! The module is behind the feature `api`, so that the WASM decoder does not carry it.

pub mod calls;
pub mod common;
pub mod envelope;
pub mod events;
pub mod ids;
pub mod methods;
pub mod model;
pub mod projections;
pub mod rich;
pub mod schema;
pub mod streams;
pub mod time;

pub use crate::version::API_VERSION;
