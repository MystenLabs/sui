// Based on toml_edit [visitor example][toml-edit-example].
//
// [toml-edit-example]: https://github.com/toml-rs/toml/blob/813ce3d757bb0276267cba1a8f8e49f5bc4b0a32/crates/toml_edit/examples/visit.rs

use toml_edit::visit_mut::{visit_table_like_kv_mut, visit_table_mut, VisitMut};
use toml_edit::{Array, DocumentMut, InlineTable, Item, KeyMut, Table, Value};

pub fn normalize_deps(toml: &mut DocumentMut) -> anyhow::Result<()> {
    Ok(todo!())
}

/// Normalize all dependency tables into the format:
///
/// ```toml
/// [dependencies]
/// dep = { version = "1.0", features = ["foo", "bar"], ... }
/// ```
///
/// leaving other tables untouched.
#[derive(Debug)]
struct NormalizeDependencyTablesVisitor {
    state: VisitState,
}

/// This models the visit state for dependency keys in a `Cargo.toml`.
///
/// Dependencies can be specified as:
///
/// ```toml
/// [dependencies]
/// dep1 = "0.2"
///
/// [build-dependencies]
/// dep2 = "0.3"
///
/// [dev-dependencies]
/// dep3 = "0.4"
///
/// [target.'cfg(windows)'.dependencies]
/// dep4 = "0.5"
///
/// # and target build- and dev-dependencies
/// ```
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum VisitState {
    /// Represents the root of the table.
    Root,
    /// Represents "dependencies", "build-dependencies" or "dev-dependencies", or the target
    /// forms of these.
    Dependencies,
    /// A table within dependencies.
    SubDependencies,
    /// Represents "target".
    Target,
    /// "target.[TARGET]".
    TargetWithSpec,
    /// Represents some other state.
    Other,
}

impl VisitState {
    /// Figures out the next visit state, given the current state and the given key.
    fn descend(self, key: &str) -> Self {
        match (self, key) {
            (
                VisitState::Root | VisitState::TargetWithSpec,
                "dependencies" | "build-dependencies" | "dev-dependencies",
            ) => VisitState::Dependencies,
            (VisitState::Root, "target") => VisitState::Target,
            (VisitState::Root | VisitState::TargetWithSpec, _) => VisitState::Other,
            (VisitState::Target, _) => VisitState::TargetWithSpec,
            (VisitState::Dependencies, _) => VisitState::SubDependencies,
            (VisitState::SubDependencies, _) => VisitState::SubDependencies,
            (VisitState::Other, _) => VisitState::Other,
        }
    }
}

impl VisitMut for NormalizeDependencyTablesVisitor {
    fn visit_table_mut(&mut self, node: &mut Table) {
        visit_table_mut(self, node);

        // The conversion from regular tables into inline ones might leave some explicit parent
        // tables hanging, so convert them to implicit.
        if matches!(self.state, VisitState::Target | VisitState::TargetWithSpec) {
            node.set_implicit(true);
        }
    }

    fn visit_table_like_kv_mut(&mut self, mut key: KeyMut<'_>, node: &mut Item) {
        let old_state = self.state;

        // Figure out the next state given the key.
        self.state = self.state.descend(key.get());

        match self.state {
            VisitState::Target | VisitState::TargetWithSpec | VisitState::Dependencies => {
                // Top-level dependency row, or above: turn inline tables into regular ones.
                if let Item::Value(Value::InlineTable(inline_table)) = node {
                    let inline_table = std::mem::replace(inline_table, InlineTable::new());
                    let table = inline_table.into_table();
                    key.fmt();
                    *node = Item::Table(table);
                }
            }
            VisitState::SubDependencies => {
                // Individual dependency: turn regular tables into inline ones.
                if let Item::Table(table) = node {
                    // Turn the table into an inline table.
                    let table = std::mem::replace(table, Table::new());
                    let inline_table = table.into_inline_table();
                    key.fmt();
                    *node = Item::Value(Value::InlineTable(inline_table));
                }
            }
            _ => {}
        }

        // Recurse further into the document tree.
        visit_table_like_kv_mut(self, key, node);

        // Restore the old state after it's done.
        self.state = old_state;
    }

    fn visit_array_mut(&mut self, node: &mut Array) {
        // Format any arrays within dependencies to be on the same line.
        if matches!(
            self.state,
            VisitState::Dependencies | VisitState::SubDependencies
        ) {
            node.fmt();
        }
    }
}
