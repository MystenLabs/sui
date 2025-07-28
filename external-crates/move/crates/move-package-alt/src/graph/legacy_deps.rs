use crate::flavor::MoveFlavor;

use super::PackageGraph;

impl<F: MoveFlavor> PackageGraph<F> {
    /// This pass adds transitive dependencies to the graph for legacy dependencies.
    /// In the old system, you did not need to explicitly depend on a package in order to refer to its
    /// name in your Move code; you would automatically inherit all of the names defined by your deps.
    /// This means that legacy packages would no longer build in the new system, so we make a pass over
    /// the dependency graph adding edges to all transitive dependencies
    fn add_legacy_transitive_edges(&mut self) {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use test_log::test;

    use crate::{
        flavor::Vanilla,
        graph::{PackageGraph, PackageInfo},
        schema::PackageName,
        test_utils::graph_builder::TestPackageGraph,
    };

    /// Return the packages in the graph, grouped by their name
    fn packages_by_name(
        graph: &PackageGraph<Vanilla>,
    ) -> BTreeMap<PackageName, PackageInfo<Vanilla>> {
        graph
            .dependencies()
            .into_iter()
            .map(|node| (node.name().clone(), node))
            .collect()
    }

    /// Root package `root` depends on `a` which depends on `b` which depends on `c`, which depends
    /// on `d`; `a`, `b`,
    /// `c`, and `d` are all legacy packages.
    ///
    /// after adding legacy transitive deps, 'a' should have direct dependencies on `c` and `d`
    #[test(tokio::test)]
    async fn modern_legacy_legacy_legacy_legacy() {
        let scenario = TestPackageGraph::new(["root"])
            .add_legacy_packages(["a", "b", "c", "d"])
            .add_deps([("root", "a"), ("a", "b"), ("b", "c"), ("c", "d")])
            .build();

        let mut graph = scenario.graph_for("root").await;

        graph.add_legacy_transitive_edges();

        let packages = packages_by_name(&graph);

        assert!(packages["a"].named_addresses().contains_key("c"));
        assert!(packages["a"].named_addresses().contains_key("d"));
        assert!(!packages["root"].named_addresses().contains_key("c"));
    }

    /// Root package `root` depends on `a` which depends on `b` which depends on `c` which depends
    /// on `d`; `a` and `c` are legacy packages.
    ///
    /// After adding legacy transitive deps, `a` should have direct dependencies on `c` and `d`
    /// (even though they "pass through" a modern package)
    #[test(tokio::test)]
    async fn modern_legacy_modern_legacy() {
        let scenario = TestPackageGraph::new(["root", "b", "d"])
            .add_legacy_packages(["a", "c"])
            .add_deps([("root", "a"), ("a", "b"), ("b", "c"), ("c", "d")])
            .build();

        let mut graph = scenario.graph_for("root").await;

        graph.add_legacy_transitive_edges();

        let packages = packages_by_name(&graph);

        assert!(packages["a"].named_addresses().contains_key("c"));
        assert!(packages["a"].named_addresses().contains_key("d"));
        assert!(!packages["b"].named_addresses().contains_key("d"));
    }

    // TODO: tests around name conflicts?
}
