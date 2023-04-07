// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/// Container for experiments in the compiler. Those can be activated
/// via the `--experiment=<name>` flag.
///
/// One can activate an experiment in a test source by using a comment as such:
/// ```
///   // experiment: <name>
/// ```
/// This can be repeated an arbitrary number of times. The test will then be run for
/// the default configuration and for each of the named experiments separately (if it is a
/// baseline test, a different baseline file will be generated each time).
///
/// Each new experiment should have a description and explicit note about its retention.
///
/// - Permanent: the experiment is available indefinitely
/// - Temporary: the experiment is intended to be removed after some time. Please document
///   the condition under which it can be removed.
pub struct Experiment();

impl Experiment {
    /// Capture source information in baseline generation when running a test. By default,
    /// during tests this is off.
    /// Retention: permanent
    pub const CAPTURE_SOURCE_INFO: &'static str = "capture-source-info";
}
