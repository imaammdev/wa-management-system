//! mms-core — domain layer shared by the API and the worker.
//!
//! Everything in this crate is tenant-scoped: queries take an `org_id` (and
//! often a `customer_id`) explicitly. There is no ambient "current tenant".

pub mod config;
pub mod crypto;
pub mod db;
pub mod error;
pub mod jobs;
pub mod models;
pub mod quota;
pub mod repo;
pub mod tenancy;

pub use config::Config;
pub use error::{Error, Result};

/// Re-exported so downstream crates don't need to pin sqlx themselves.
pub use sqlx::PgPool;
