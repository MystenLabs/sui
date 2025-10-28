use petgraph::visit::EdgeRef;
use thiserror::Error;

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

Alternatively, if you want to refer to `{actual_dep_name}` as `{local_dep_name}`, add a `rename-from` field to the dependency:

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
            } else {
                if local_dep_name != actual_dep_name {
                    return Err(RenameError::MismatchedNames {
                        local_dep_name,
                        dep_location: dep.as_ref().abbreviated(),
                        actual_dep_name,
                    });
                }
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

        Alternatively, if you want to refer to `b` as `c`, add a `rename-from` field to the dependency:

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
