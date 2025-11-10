use petgraph::visit::EdgeRef;
use thiserror::Error;
use tracing::debug;

use crate::flavor::MoveFlavor;

use super::PackageGraph;

// TODO: these could/should be manifest errors with locations
#[derive(Debug, Error)]
pub enum RenameError {
    #[error("{0}")]
    RenameFromError(String),

    #[error(
        "In Move.toml, the dependency `{local_dep_name}` has `rename-from = \"{specified_rename_from}\"`, but the referred package is named `{actual_dep_name}`. Change the `rename-from` field to `{actual_dep_name}`."
    )]
    RenameFromMismatch {
        local_dep_name: String,
        specified_rename_from: String,
        actual_dep_name: String,
    },

    #[error(
        "In Move.toml, the dependency `{actual_dep_name}` has a `rename-from` field, but the referred package is already named `{actual_dep_name}`. Remove the unnecessary `rename-from` field."
    )]
    RenameFromUnneccessary { actual_dep_name: String },

    #[error(
"In Move.toml, the dependency `{local_dep_name}` refers to a package named `{actual_dep_name}`. Consider renaming the dependency to `{actual_dep_name}`:

    {actual_dep_name} = {{ {dep_location}, ... }}

Alternatively, if you want to refer to `{actual_dep_name}` as `{local_dep_name}` in your source code, add a `rename-from` field to the dependency:

    {local_dep_name} = {{ {dep_location}, rename-from = \"{actual_dep_name}\", ... }}

"
    )]
    MismatchedNames {
        local_dep_name: String,
        dep_location: String,
        actual_dep_name: String,
    },

    #[error(
        "In Move.toml, the dependency `{local_dep_name}` refers to a package named `{actual_dep_name}`. Consider renaming the dependency to `{actual_dep_name}`:

            {actual_dep_name} = {{ {dep_location}, ... }}

        "
    )]
    LegacyMismatchedNames {
        local_dep_name: String,
        dep_location: String,
        actual_dep_name: String,
    },
}

type RenameResult<T> = Result<T, RenameError>;

impl<F: MoveFlavor> PackageGraph<F> {
    /// Ensures that each `name` fields in all (direct) dependencies is the same as the name
    /// given to it in the depending package, unless `rename-from` is specified.
    pub fn check_rename_from(&self) -> RenameResult<()> {
        if self.root_package().is_legacy() {
            self.check_legacy_rename_from()
        } else {
            self.check_modern_rename_from()
        }
    }

    /// For each dep `dep = { ..., rename-from = "..." }` ensure that the rename-from field (if
    /// present) and the dependency's target name are consistent
    fn check_modern_rename_from(&self) -> RenameResult<()> {
        for edge in self.inner.edges(self.root_index) {
            let dep = &edge.weight();

            let target_pkg = self.inner[edge.target()].clone();
            let local_dep_name = dep.name().to_string();
            let actual_dep_name = target_pkg.name().to_string();

            if let Some(rename) = dep.rename_from() {
                if local_dep_name == actual_dep_name {
                    return Err(RenameError::RenameFromUnneccessary { actual_dep_name });
                }

                if rename.to_string() != actual_dep_name {
                    return Err(RenameError::RenameFromMismatch {
                        local_dep_name,
                        specified_rename_from: rename.to_string(),
                        actual_dep_name,
                    });
                }
            } else if local_dep_name != actual_dep_name {
                return Err(RenameError::MismatchedNames {
                    local_dep_name,
                    dep_location: dep.as_ref().abbreviated(),
                    actual_dep_name,
                });
            }
        }

        Ok(())
    }

    /// For each dep `Dep = { ... }`, ensure that referred package either has variable name `Dep`
    /// or legacy package name `Dep`. This allows a transitional state where a legacy package uses
    /// the modern name for a legacy dependency.
    fn check_legacy_rename_from(&self) -> RenameResult<()> {
        for edge in self.inner.edges(self.root_index) {
            let target_pkg = self.inner[edge.target()].clone();
            let local_dep_name = edge.weight().name().to_string();

            let actual_dep_name = target_pkg.name().to_string();
            if local_dep_name == actual_dep_name {
                continue;
            }

            if let Some(legacy) = &target_pkg.legacy_data
                && legacy.normalized_legacy_name.to_string() == local_dep_name
            {
                continue;
            }

            let dep_location = edge.weight().as_ref().abbreviated();

            debug!("local name: {local_dep_name}");
            debug!("actual name: {actual_dep_name}");
            if let Some(legacy) = &target_pkg.legacy_data {
                debug!("legacy name: {}", legacy.normalized_legacy_name);
            }
            return Err(RenameError::LegacyMismatchedNames {
                local_dep_name,
                dep_location,
                actual_dep_name,
            });
        }

        Ok(())
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
    /// rename-from check should fail on a, indicating that `a.c` should have `rename-from = "b"`
    /// rename-from check on root should succeed, since we only check the root package
    #[test(tokio::test)]
    async fn test_no_rename_from() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_deps([("root", "a")])
            .add_dep("a", "b", |dep| dep.name("c"))
            .build();

        scenario
            .graph_for("root")
            .await
            .check_rename_from()
            .unwrap();

        assert_snapshot!(scenario.graph_for("a").await.check_rename_from().unwrap_err().to_string(), @r###"
        In Move.toml, the dependency `c` refers to a package named `b`. Consider renaming the dependency to `b`:

            b = { local = "../b", ... }

        Alternatively, if you want to refer to `b` as `c` in your source code, add a `rename-from` field to the dependency:

            c = { local = "../b", rename-from = "b", ... }
        "###);
    }

    /// `a` depends on `b`; the dependency from `a` to `b` is named `c`,
    /// but the rename-from field says `rename-from = "d"`.
    ///
    /// rename-from check should fail, indicating that `a.c` should have `rename-from = "b"`
    #[test(tokio::test)]
    async fn test_wrong_rename_from() {
        let scenario = TestPackageGraph::new(["a", "b"])
            .add_dep("a", "b", |dep| dep.name("c").rename_from("d"))
            .build();

        assert_snapshot!(scenario.graph_for("a").await.check_rename_from().unwrap_err().to_string(), @r###"In Move.toml, the dependency `c` has `rename-from = "d"`, but the referred package is named `b`. Change the `rename-from` field to `b`."###);
    }

    /// `a` depends on `b`; the dependency from `a` to `b` has `rename-from = "b"`
    ///
    /// rename-from check should fail, indicating that the rename-from is unnecessary
    #[test(tokio::test)]
    async fn test_unnecessary_rename_from() {
        let scenario = TestPackageGraph::new(["a", "b"])
            .add_dep("a", "b", |dep| dep.rename_from("b"))
            .build();

        assert_snapshot!(scenario.graph_for("a").await.check_rename_from().unwrap_err().to_string(), @"In Move.toml, the dependency `b` has a `rename-from` field, but the referred package is already named `b`. Remove the unnecessary `rename-from` field.");
    }

    /// `root` depends on `a` which has an external dependency on `b`; the dependency from `a` to
    /// `b` is named `c`, and there is no rename-from field.
    ///
    /// rename-from check should fail, but the error message should contain the external dependency
    /// rather than the resolved dependency
    #[test(tokio::test)]
    #[ignore] // TODO
    async fn test_external_rename_error() {
        let scenario = TestPackageGraph::new(["a", "b"])
            .add_dep(
                "a",
                "b",
                |dep| dep.name("c"), /* TODO .make_external()*/
            )
            .build();

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

    /// modern package `bat` depends on legacy package `sui` which has legacy name `Sui` (capital S).
    ///
    /// The dependency is named `Sui`. This should be disallowed, since the legacy name shouldn't
    /// appear in a modern manifest
    #[test(tokio::test)]
    async fn modern_uses_legacy_name() {
        let scenario = TestPackageGraph::new(["bat"])
            .add_package("sui", |pkg| pkg.set_legacy().set_legacy_name("Sui"))
            .add_dep("bat", "sui", |dep| dep.name("Sui"))
            .build();

        assert_snapshot!(scenario.graph_for("bat").await.check_rename_from().unwrap_err(), @r###"
        In Move.toml, the dependency `Sui` refers to a package named `sui`. Consider renaming the dependency to `sui`:

            sui = { local = "../sui", ... }

        Alternatively, if you want to refer to `sui` as `Sui` in your source code, add a `rename-from` field to the dependency:

            Sui = { local = "../sui", rename-from = "sui", ... }
        "###);
    }

    /// modern package `foo` depends on legacy package `std` which has legacy name `MoveStdlib`.
    ///
    /// The dependency is named `MoveStdlib` but has `rename-from = "std"`; this should be allowed
    /// (although dumb)
    #[test(tokio::test)]
    async fn modern_uses_legacy_name_with_rename() {
        let scenario = TestPackageGraph::new(["foo"])
            .add_package("std", |pkg| pkg.set_legacy().set_legacy_name("MoveStdlib"))
            .add_dep("foo", "std", |dep| {
                dep.name("MoveStdlib").rename_from("std")
            })
            .build();

        scenario.graph_for("foo").await.check_rename_from().unwrap();
    }

    /// modern package `bar` depends on legacy package `std` which has legacy name `MoveStdlib`.
    ///
    /// The dependency is named `foo` but has `rename-from = "MoveStdlib"`; this should fail
    /// because you need to use the modern name in the rename-from field
    #[test(tokio::test)]
    async fn modern_uses_legacy_name_in_rename() {
        let scenario = TestPackageGraph::new(["bar"])
            .add_package("std", |pkg| pkg.set_legacy().set_legacy_name("MoveStdlib"))
            .add_dep("bar", "std", |dep| {
                dep.name("foo").rename_from("MoveStdlib")
            })
            .build();

        assert_snapshot!(scenario.graph_for("bar").await.check_rename_from().unwrap_err(), @r###"In Move.toml, the dependency `foo` has `rename-from = "MoveStdlib"`, but the referred package is named `std`. Change the `rename-from` field to `std`."###);
    }

    /// legacy package `legacy` uses legacy package `std` which has legacy name `MoveStdlib`;
    ///
    /// The dependency is named `MoveStdlib`; this should succeed
    #[test(tokio::test)]
    async fn legacy_legacy_name() {
        let scenario = TestPackageGraph::new(["root"])
            .add_package("legacy", |pkg| pkg.set_legacy())
            .add_package("std", |pkg| pkg.set_legacy().set_legacy_name("MoveStdlib"))
            .add_dep("legacy", "std", |dep| dep.name("MoveStdlib"))
            .build();

        scenario
            .graph_for("legacy")
            .await
            .check_rename_from()
            .unwrap();
    }

    /// legacy package `legacy` uses legacy package `std` which has legacy name `MoveStdlib`;
    ///
    /// The dependency is named `std`; this should succeed to aid transition to the new system
    #[test(tokio::test)]
    async fn legacy_modern_name() {
        let scenario = TestPackageGraph::new(["root"])
            .add_package("legacy", |pkg| pkg.set_legacy())
            .add_package("std", |pkg| pkg.set_legacy().set_legacy_name("MoveStdlib"))
            .add_dep("legacy", "std", |dep| dep.name("std"))
            .build();

        scenario
            .graph_for("legacy")
            .await
            .check_rename_from()
            .unwrap();
    }

    /// legacy package `baz` uses legacy package `malformed` which has legacy name `weird-name`;
    ///
    /// The dependency is named `weird-name`; this should succeed
    #[test(tokio::test)]
    async fn legacy_malformed_name() {
        let scenario = TestPackageGraph::new(["root"])
            .add_package("baz", |pkg| pkg.set_legacy())
            .add_package("malformed", |pkg| {
                pkg.set_legacy().set_legacy_name("weird-name")
            })
            .add_dep("baz", "malformed", |dep| dep.name("weird-name"))
            .build();

        scenario.graph_for("baz").await.check_rename_from().unwrap();
    }

    /// legacy package `legacy2` depends on legacy package `malformed` which has legacy name
    /// `weird-name`.
    ///
    /// The dependency is named `Wrong`. This should fail because of the rename-from check
    #[test(tokio::test)]
    async fn legacy_wrong_name() {
        let scenario = TestPackageGraph::new(["root"])
            .add_package("legacy2", |pkg| pkg.set_legacy())
            .add_package("malformed", |pkg| {
                pkg.set_legacy().set_legacy_name("weird-name")
            })
            .add_dep("legacy2", "malformed", |dep| dep.name("Wrong"))
            .build();

        assert_snapshot!(scenario.graph_for("legacy2").await.check_rename_from().unwrap_err(), @r###"
        In Move.toml, the dependency `Wrong` refers to a package named `malformed`. Consider renaming the dependency to `malformed`:

                    malformed = { local = "../malformed", ... }
        "###);
    }
}
