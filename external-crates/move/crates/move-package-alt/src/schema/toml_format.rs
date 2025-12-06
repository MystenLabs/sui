use toml_edit::{
    Array, ArrayOfTables, DocumentMut, InlineTable, Item, KeyMut, Table, Value,
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
            key.fmt();

            if let Item::Value(Value::InlineTable(inline_table)) = node {
                let inline_table = std::mem::replace(inline_table, InlineTable::new());
                let table = inline_table.into_table();
                *node = Item::Table(table);
            } else if let Item::Value(Value::Array(array)) = node {
                if array.iter().all(|item| item.is_inline_table()) {
                    let array = std::mem::replace(array, Array::new());
                    let mut aot = ArrayOfTables::new();
                    for item in array.into_iter() {
                        let Value::InlineTable(table) = item else {
                            panic!("we checked that all elements are inline tables")
                        };
                        aot.push(table.into_table());
                    }
                    *node = Item::ArrayOfTables(aot);
                }
                return;
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
    use std::str::FromStr;

    use indoc::indoc;
    use insta::assert_snapshot;
    use test_log::test;

    use super::{expand_toml, flatten_toml};

    /// expand_toml works
    #[test]
    fn expand() {
        let mut toml: toml_edit::DocumentMut = toml_edit::DocumentMut::from_str(indoc!(
            r#"
            a.b.c = { d = { f = "g" } }
            [foo]
            bar = { baz = "quux" }
            "#
        ))
        .unwrap();

        expand_toml(&mut toml);

        assert_snapshot!(toml.to_string(), @r###"
        [a.b.c.d]
        f = "g"

        [foo.bar]
        baz = "quux"
        "###);
    }

    /// flatten_toml works on the whole document
    #[test]
    fn flatten() {
        let mut toml: toml_edit::DocumentMut = toml_edit::DocumentMut::from_str(indoc!(
            r#"
            a.b.c = { d = { f = "g" } }
            [x.y]
            bar = { baz = "quux" }
            "#
        ))
        .unwrap();

        flatten_toml(toml.as_item_mut());

        assert_snapshot!(toml.to_string(), @r###"
        a = { b = { c = { d = { f = "g" } } } }
        x = { y = { bar = { baz = "quux" } } }
        "###);
    }

    /// expanding then flattening particular sections does the right thing
    #[test]
    fn expand_flatten() {
        let mut toml: toml_edit::DocumentMut = toml_edit::DocumentMut::from_str(indoc!(
            r#"
            a.b.c = { d = { f = "g" } }
            [x.y]
            bar = { baz = "quux" }
            "#
        ))
        .unwrap();

        expand_toml(&mut toml);
        flatten_toml(
            toml["a"]
                .as_table_like_mut()
                .unwrap()
                .get_mut("b")
                .unwrap()
                .get_mut("c")
                .unwrap(),
        );
        flatten_toml(toml["x"].as_table_like_mut().unwrap().get_mut("y").unwrap());

        assert_snapshot!(toml.to_string(), @r###"
        [a.b.c]
        d = { f = "g" }
        [x.y]
        bar = { baz = "quux" }
        "###);
    }
}
