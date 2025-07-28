//! This pass adds transitive dependencies to the graph for legacy dependencies.
//! In the old system, you did not need to explicitly depend on a package in order to refer to its
//! name in your Move code; you would automatically inherit all of the names defined by your deps.
//! This means that legacy packages would no longer build in the new system, so we make a pass over
//! the dependency graph adding edges to all transitive dependencies

use crate::flavor::MoveFlavor;

use super::PackageGraph;

impl<F: MoveFlavor> PackageGraph<F> {}

#[cfg(test)]
mod tests {
    use test_log::test;

    use crate::test_utils::graph_builder::TestPackageGraph;

    /// Root package `root` depends on `a` which depends on `b` which depends on `c`; `a`, `b`, and
    /// `c` are all legacy packages.
    ///
    /// after adding legacy transitive deps, 'a' should have a direcy dependency on `c`
    #[test]
    fn modern_legacy_legacy_legacy() {
        let scenario = TestPackageGraph::new(["root"]).add_legacy_packages(["a", "b", "c"]);
    }
}
