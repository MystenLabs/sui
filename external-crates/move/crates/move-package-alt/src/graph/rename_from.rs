use indoc::formatdoc;
use petgraph::visit::EdgeRef;
use thiserror::Error;

use crate::{
    dependency::PinnedDependencyInfo, flavor::MoveFlavor, package::Package, schema::PackageName,
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
            let actual_name = self.inner[edge.target()].name();

            if expected_name != actual_name {
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
}
