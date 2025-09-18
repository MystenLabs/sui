// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This crate provides a flexible and extensible compilation system for Move packages,
//!
//! # Key Features
//!
//! - **Customizable Compilation Pipeline**: Support for custom compiler drivers and hooks
//! - **Multi-Flavor Support**: Generic over Move language variants
//! - **Comprehensive Artifact Generation**: Bytecode, documentation, and metadata
//! - **Flexible Build Configuration**: Fine-grained control over compilation settings
//!
//! # Main Entry Points
//!
//! - [`compile_package`]: Compile a Move package from a filesystem path
//! - [`compile_from_root_package`]: Compile from an already-loaded package structure
//! - [`BuildPlan`]: Orchestrates the compilation process and manages dependencies
//! - [`BuildConfig`]: Configures the compilation settings and flags

/// Build configuration and compilation settings type.
///
/// This module provides the [`BuildConfig`] structure which controls all aspects of the
/// compilation process including output directories, compilation flags, error handling,
/// and language-specific settings.
pub mod build_config;

/// Build plan orchestration and compilation execution.
///
/// This module contains the [`BuildPlan`] which orchestrates the entire compilation
/// process, manages package dependencies, and provides hooks for custom compilation
/// drivers to modify the compilation pipeline.
pub mod build_plan;

/// Core compilation functionality and entry points.
///
/// This module provides the main compilation functions that serve as entry points
/// to the compilation pipeline, handling both path-based and pre-loaded package
/// compilation scenarios.
pub mod compilation;

/// In-memory representation of compiled Move packages.
///
/// This module defines the [`CompiledPackage`] structure which represents the
/// final compiled output including bytecode, metadata, and documentation for
/// both the root package and its dependencies.
pub mod compiled_package;

/// Documentation generation and processing utilities.
///
/// This module handles the generation and management of Move package documentation,
/// including docstrings extraction and documentation artifact creation.
pub mod documentation;

/// On-disk package layout and structure definitions.
///
/// This module defines the standard directory structure and file organization
/// for compiled Move packages, including the layout of compiled modules, dependencies, and other
/// artifacts.
pub mod layout;

/// Linting configuration and flag management.
///
/// This module provides types and utilities for configuring the Move linter,
/// including warning levels, error promotion, and lint rule customization.
pub mod lint_flag;

/// Migration related functionality.
pub mod migrate;

/// Move model building and analysis integration.
pub mod model_builder;

/// On-disk compiled package representation and serialization.
///
/// This module handles the serialization and deserialization of compiled packages
/// to and from disk, managing the persistent storage of compilation artifacts.
pub mod on_disk_package;

/// Shared utilities and common functionality, mostly for getting the right paths for the build
/// configuration.
pub mod shared;

/// Source code discovery and file management.
///
/// This module provides functionality for discovering and managing Move source files,
/// including file traversal, pattern matching.
pub mod source_discovery;

pub use compilation::compile_from_root_package;
pub use compilation::compile_package;
