use indoc::formatdoc;
use petgraph::visit::EdgeRef;
use thiserror::Error;

use crate::{
    compatibility::legacy::LegacyData, dependency::PinnedDependencyInfo, flavor::MoveFlavor,
    package::Package, schema::PackageName,
};

use super::PackageGraph;

#[derive(Debug, Error)]
pub enum RenameError {
    #[error("{0}")]
    RenameFromError(String),
}

type RenameResult<T> = Result<T, RenameError>;

impl<F: MoveFlavor> PackageGraph<F> {
    /// Ensures that the `name` fields in all (transitive) dependencies is the same as the name
    /// given to it in the depending package, unless `rename-from` is specified.
    pub fn check_rename_from(&self) -> RenameResult<()> {
        for edge in self.inner.edge_references() {
            let dep = &edge.weight().dep;

            let expected_name = dep.rename_from().as_ref().unwrap_or(&edge.weight().name);
            let origin_pkg = self.inner[edge.source()].clone();
            let target_pkg = self.inner[edge.target()].clone();

            // Modern packages: If there's a name missmatch, we error
            // Legacy packages: If there's a name missmatch and there's also a missmatch with the
            // legacy name, we fail again.
            if expected_name != target_pkg.name()
                && (!origin_pkg.is_legacy()
                    || !is_legacy_match(&target_pkg.legacy_data, expected_name))
            {
                return Err(RenameError::new(
                    &self.inner[edge.source()],
                    &self.inner[edge.target()],
                    &edge.weight().name,
                    dep,
                ));
            }
        }

        Ok(())
    }
}

/// Checks that for a given package `pkg`, if it's legacy, the expected name
/// matches the normalized legacy name.
fn is_legacy_match(legacy_data: &Option<LegacyData>, expected_name: &PackageName) -> bool {
    if let Some(legacy_data) = legacy_data {
        &legacy_data.normalized_legacy_name == expected_name
    } else {
        false
    }
}

impl RenameError {
    /// Construct a `RenameError` with a descriptive message
    fn new<F: MoveFlavor>(
        source: &Package<F>,
        target: &Package<F>,
        dep_name: &PackageName,
        dep: &PinnedDependencyInfo,
    ) -> Self {
        // example: a contains `dep_name = { local = "b_path", rename=from = "b_name" }`

        // in example: path is "root -> a"
        let path = "<TODO>";

        // in example: source_name is "a"
        let source_name = source.name();

        // in example: target_name is "b"
        let target_name = target.name();

        // in example: expected_target_name is "b_name"
        let _expected_target_name = dep.rename_from().as_ref().unwrap_or(dep_name);

        // in example: dep_location is `local = "b_path"`
        let dep_location = "<TODO>";

        // in example: rendered_dep is `dep = { local = "b_path", rename-from = "b_name" }`
        // TODO: use spans / diagnostics here instead; without that, `mvr` diagnostics will show
        //       the resolved dep rather than the original dep
        let rendered_dep = if let Some(rename_from) = dep.rename_from() {
            format!(r#"{dep_name} = {{ {dep_location}, rename-from = "{rename_from}", ... }}"#)
        } else {
            format!("{dep_name} = {{ {dep_location}, ... }}")
        };

        Self::RenameFromError(formatdoc!(
            "Package `{source_name}` (included from {path}) has a dependency `{rendered_dep}`, but the package at \
            `{dep_location}` has `name = \"{target_name}\"`. If you intend to rename `{target_name}` to `{dep_name}` in `{source_name}`, add \
            `rename-from = \"{target_name}\"` to the dependency in the `Move.toml` for `a`:

                {dep_name} = {{ {dep_location}, rename-from = \"{target_name}\", ... }}\n"
        ))
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use test_log::test;

    use crate::test_utils::graph_builder::TestPackageGraph;

    /// `root` depends on `a` which depends on `b`; the dependency from `a` to `b` is named `c`,
    /// and has specified `rename-from = "c"`.
    ///
    /// graphs rooted at both `root` and `a` should pass the rename-from check
    #[test(tokio::test)]
    async fn test_with_rename_from() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_deps([("root", "a")])
            .add_dep("a", "b", |dep| dep.name("c").rename_from("b"))
            .build();

        scenario
            .graph_for("root")
            .await
            .check_rename_from()
            .unwrap();
        scenario.graph_for("a").await.check_rename_from().unwrap();
    }

    /// `root` depends on `a` which depends on `b`; the dependency from `a` to `b` is named `c`,
    /// but there is no rename-from field.
    ///
    /// rename-from check should fail, indicating that `a.c` should have `rename-from = "b"`
    #[test(tokio::test)]
    async fn test_no_rename_from() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_deps([("root", "a")])
            .add_dep("a", "b", |dep| dep.name("c"))
            .build();

        assert_snapshot!(scenario.graph_for("root").await.check_rename_from().unwrap_err().to_string(), @r###"
        Package `a` (included from <TODO>) has a dependency `c = { <TODO>, ... }`, but the package at `<TODO>` has `name = "b"`. If you intend to rename `b` to `c` in `a`, add `rename-from = "b"` to the dependency in the `Move.toml` for `a`:

            c = { <TODO>, rename-from = "b", ... }
        "###);
        assert_snapshot!(scenario.graph_for("a").await.check_rename_from().unwrap_err().to_string(), @r###"
        Package `a` (included from <TODO>) has a dependency `c = { <TODO>, ... }`, but the package at `<TODO>` has `name = "b"`. If you intend to rename `b` to `c` in `a`, add `rename-from = "b"` to the dependency in the `Move.toml` for `a`:

            c = { <TODO>, rename-from = "b", ... }
        "###);
    }

    /// `root` depends on `a` which depends on `b`; the dependency from `a` to `b` is named `c`,
    /// but the rename-from field says `rename-from = "d"`.
    ///
    /// rename-from check should fail, indicating that `a.c` should have `rename-from = "b"`
    #[test(tokio::test)]
    async fn test_wrong_rename_from() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_deps([("root", "a")])
            .add_dep("a", "b", |dep| dep.name("c").rename_from("d"))
            .build();

        assert_snapshot!(scenario.graph_for("root").await.check_rename_from().unwrap_err().to_string(), @r###"
        Package `a` (included from <TODO>) has a dependency `c = { <TODO>, rename-from = "d", ... }`, but the package at `<TODO>` has `name = "b"`. If you intend to rename `b` to `c` in `a`, add `rename-from = "b"` to the dependency in the `Move.toml` for `a`:

            c = { <TODO>, rename-from = "b", ... }
        "###);
        assert_snapshot!(scenario.graph_for("a").await.check_rename_from().unwrap_err().to_string(), @r###"
        Package `a` (included from <TODO>) has a dependency `c = { <TODO>, rename-from = "d", ... }`, but the package at `<TODO>` has `name = "b"`. If you intend to rename `b` to `c` in `a`, add `rename-from = "b"` to the dependency in the `Move.toml` for `a`:

            c = { <TODO>, rename-from = "b", ... }
        "###);
    }

    /// `root` depends on `a` which has an external dependency on `b`; the dependency from `a` to
    /// `b` is named `c`, and there is no rename-from field.
    ///
    /// rename-from check should fail, but the error message should contain the external dependency
    /// rather than the resolved dependency
    #[test(tokio::test)]
    #[ignore] // TODO
    async fn test_external_rename_error() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_deps([("root", "a")])
            .add_dep("a", "b", |dep| dep.name("c").make_external())
            .build();

        assert_snapshot!(scenario.graph_for("root").await.check_rename_from().unwrap_err().to_string(), @"TODO");
        assert_snapshot!(scenario.graph_for("a").await.check_rename_from().unwrap_err().to_string(), @"TODO");
    }

    #[test(tokio::test)]
    /// TODO: Add a mermaid diagram
    async fn test_modern_using_legacy_framework() {
        let scenario = TestPackageGraph::new(["root"])
            .add_package("std", |pkg| pkg.set_legacy().set_legacy_name("MoveStdLib"))
            .add_package("sui", |pkg| pkg.set_legacy().set_legacy_name("Sui"))
            .add_package("sui_system", |pkg| {
                pkg.set_legacy().set_legacy_name("SuiSystem")
            })
            .add_deps([("root", "std")])
            .add_dep("root", "sui", |dep| dep.name("my_sui").rename_from("sui"))
            .add_dep("root", "sui_system", |dep| {
                dep.name("my_sui_system").rename_from("sui_system")
            })
            .add_dep("sui", "std", |dep| dep.name("MoveStdLib"))
            // legacy -> legacy case (SuiSystem -> MoveStdLib (std)
            .add_dep("sui_system", "sui", |dep| dep.name("Sui"))
            .add_dep("sui_system", "std", |dep| dep.name("MoveStdLib"))
            .build();

        scenario
            .graph_for("root")
            .await
            .check_rename_from()
            .unwrap();

        scenario
            .graph_for("sui_system")
            .await
            .check_rename_from()
            .unwrap();
    }

    #[test(tokio::test)]
    async fn test_modern_to_legacy_not_allowed_behaviours() {
        let scenario = TestPackageGraph::new(["foo", "bat", "bar", "baz"])
            .add_package("legacy", |pkg| pkg.set_legacy().set_legacy_name("Legacy"))
            .add_package("legacy2", |pkg| pkg.set_legacy().set_legacy_name("Legacy2"))
            .add_package("legacy3", |pkg| pkg.set_legacy().set_legacy_name("Legacy3"))
            .add_package("sui", |pkg| pkg.set_legacy().set_legacy_name("Sui"))
            .add_package("std", |pkg| pkg.set_legacy().set_legacy_name("MoveStdLib"))
            .add_package("malformed", |pkg| {
                pkg.set_legacy().set_legacy_name("weird-input")
            })
            // 1. (FAIL) Cannot use the legacy name in the left side assignment
            .add_dep("bat", "sui", |dep| dep.name("Sui"))
            // 2. (OK) Can use the "modern" name in the rename-from (even if we name it as the legacy name)
            .add_dep("foo", "std", |dep| {
                dep.name("MoveStdLib").rename_from("std")
            })
            // 3. (FAIL) Cannot use the legacy name in the rename-from
            .add_dep("bar", "std", |dep| {
                dep.name("foo").rename_from("MoveStdLib")
            })
            // 4. (OK) Legacy packages CAN use legacy names freely!
            .add_dep("legacy", "std", |dep| dep.name("MoveStdLib"))
            // 5. (OK) Can use a malformed legacy name (as the system normalizes)
            .add_dep("baz", "malformed", |dep| dep.name("malformed"))
            // 6. (FAIL) Cannot accept wrong names for deps
            .add_dep("legacy2", "std", |dep| dep.name("Wrong"))
            .build();

        // 1.
        let _ = scenario.graph_for("bat").await.check_rename_from().is_err();

        // 2.
        scenario.graph_for("foo").await.check_rename_from().unwrap();

        // 3.
        let _ = scenario.graph_for("bar").await.check_rename_from().is_err();

        // 4.
        scenario
            .graph_for("legacy")
            .await
            .check_rename_from()
            .unwrap();

        // 5.
        scenario.graph_for("baz").await.check_rename_from().unwrap();

        // 6.
        let _ = scenario
            .graph_for("legacy2")
            .await
            .check_rename_from()
            .is_err();
    }
}
