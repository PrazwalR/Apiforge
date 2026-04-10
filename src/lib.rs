//! Apiforge core library.
//!
//! This crate powers the `apiforge` CLI and exposes building blocks for:
//! - loading and validating release configuration,
//! - orchestrating release steps with rollback support,
//! - integrating with Git, Docker, Kubernetes, AWS, and GitHub.
//!
//! Most users interact through the CLI; these modules are useful for testing,
//! extension, and embedding release automation flows.

/// CLI argument definitions and command payloads.
pub mod cli;
/// Configuration models and validation logic for `apiforge.toml`.
pub mod config;
/// Typed error hierarchy used across the release pipeline.
pub mod error;
/// Service clients for external systems (Git, Docker, Kubernetes, AWS, GitHub).
pub mod integrations;
/// Reusable helpers (semver, templates, retry, sanitization, env resolution).
pub mod utils;

/// Audit trail store and release history record types.
pub mod audit;
/// Release orchestrator that validates, executes, and rolls back step pipelines.
pub mod orchestrator;
/// Terminal output and status rendering helpers.
pub mod output;
/// Step abstraction and concrete pipeline steps.
pub mod steps;
