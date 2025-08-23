use toml_edit::{
    DocumentMut, InlineTable, Item, KeyMut, Table, Value,
    visit_mut::{self, VisitMut},
};

pub trait RenderToml {
    fn render_as_toml(&self) -> String;
}

/// Replace every inline table in [toml] with an implicit standard table (implicit tables are not
/// included if they have no keys directly inside them)
pub fn expand_toml(toml: &mut DocumentMut) {
    struct Expander;

    impl VisitMut for Expander {
        fn visit_table_mut(&mut self, table: &mut Table) {
            table.set_implicit(true);
            visit_mut::visit_table_mut(self, table);
        }

        fn visit_table_like_kv_mut(&mut self, mut key: KeyMut<'_>, node: &mut Item) {
            if let Item::Value(Value::InlineTable(inline_table)) = node {
                let inline_table = std::mem::replace(inline_table, InlineTable::new());
                let table = inline_table.into_table();
                key.fmt();
                *node = Item::Table(table);
            }
            visit_mut::visit_table_like_kv_mut(self, key, node);
        }
    }

    let mut visitor = Expander;
    visitor.visit_document_mut(toml);
}

/// Replace every table in [toml] with a non-implicit inline table.
pub fn flatten_toml(toml: &mut Item) {
    struct Inliner;

    impl VisitMut for Inliner {
        fn visit_table_mut(&mut self, table: &mut Table) {
            table.set_implicit(false);
            visit_mut::visit_table_mut(self, table);
        }

        fn visit_table_like_kv_mut(&mut self, mut key: KeyMut<'_>, node: &mut Item) {
            if let Item::Table(table) = node {
                let table = std::mem::replace(table, Table::new());
                let inline_table = table.into_inline_table();
                key.fmt();
                *node = Item::Value(Value::InlineTable(inline_table));
            }
        }
    }

    let mut visitor = Inliner;
    visitor.visit_item_mut(toml);
}

#[cfg(test)]
mod tests {
    use test_log::test;

    #[test]
    fn expand() {
        todo!()
    }

    #[test]
    fn flatten() {
        todo!()
    }

    ///
    #[test]
    fn expand_flatten() {
        todo!()
    }
}
