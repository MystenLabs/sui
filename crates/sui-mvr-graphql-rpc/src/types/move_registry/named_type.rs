// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::str::FromStr;

use async_graphql::Context;
use futures::future;
use regex::{Captures, Regex};
use sui_types::{base_types::ObjectID, TypeTag};

use crate::{data::package_resolver::PackageResolver, error::Error};

use super::{
    error::MoveRegistryError,
    named_move_package::NamedMovePackage,
    on_chain::{VersionedName, VERSIONED_NAME_UNBOUND_REG},
};

pub(crate) struct NamedType;

impl NamedType {
    /// Queries a type by the given name.
    /// Name should be a valid type tag, with move names in it in the format `app@org::type::Type`.
    /// For nested type params, we just follow the same pattern e.g. `app@org::type::Type<app@org::type::AnotherType, u64>`.
    pub(crate) async fn query(
        ctx: &Context<'_>,
        name: &str,
        checkpoint_viewed_at: u64,
    ) -> Result<TypeTag, Error> {
        let resolver: &PackageResolver = ctx.data_unchecked();
        // we do not de-duplicate the names here, as the dataloader will do this for us.
        let names = Self::parse_names(name)?;

        // Gather all the requests to resolve the names.
        let names_to_resolve = names
            .iter()
            .map(|x| NamedMovePackage::query(ctx, x, checkpoint_viewed_at))
            .collect::<Vec<_>>();

        // now we resolve all the names in parallel (data-loader will do the proper de-duplication / batching for us)
        // also the `NamedMovePackage` query will re-validate the names (including max length, which is not checked on the regex).
        let results = future::try_join_all(names_to_resolve).await?;

        // now let's create a hashmap with {name: MovePackage}
        let mut name_package_id_mapping = HashMap::new();

        for (name, result) in names.into_iter().zip(results.into_iter()) {
            let Some(package) = result else {
                return Err(Error::MoveNameRegistry(MoveRegistryError::NameNotFound(
                    name,
                )));
            };
            name_package_id_mapping.insert(name, package.native.id());
        }

        let correct_type_tag: String = Self::replace_names(name, &name_package_id_mapping)?;

        let tag = TypeTag::from_str(&correct_type_tag)
            .map_err(|e| Error::Client(format!("bad type: {e}")))?;

        resolver
            .canonical_type(tag)
            .await
            .map_err(|e| Error::Internal(format!("Failed to retrieve type: {e}")))
    }

    /// Is this already caught by the global limits?
    /// This parser just extracts all names from a type tag, and returns them
    /// We do not care about de-duplication, as the dataloader will do this for us.
    /// The goal of replacing all of them with `0x0` is to make sure that the type tag is valid
    /// so when replaced with the move name package addresses, it'll also be valid.
    fn parse_names(name: &str) -> Result<Vec<String>, Error> {
        let mut names = vec![];
        let struct_tag = VERSIONED_NAME_UNBOUND_REG.replace_all(name, |m: &regex::Captures| {
            // SAFETY: we know that the regex will always have a match on position 0.
            let name = m.get(0).unwrap().as_str();

            if VersionedName::from_str(name).is_ok() {
                names.push(name.to_string());
                "0x0".to_string()
            } else {
                name.to_string()
            }
        });

        // We attempt to parse the type_tag with these replacements, to make sure there are no other
        // errors in the type tag (apart from the move names). That protects us from unnecessary
        // queries to resolve .move names, for a type tag that will be invalid anyway.
        TypeTag::from_str(&struct_tag).map_err(|e| Error::Client(format!("bad type: {e}")))?;

        Ok(names)
    }

    /// This function replaces all the names in the type tag with their corresponding MovePackage address.
    /// The names are guaranteed to be the same and exist (as long as this is called in sequence),
    /// since we use the same parser to extract the names.
    fn replace_names(type_name: &str, names: &HashMap<String, ObjectID>) -> Result<String, Error> {
        let struct_tag_str = replace_all_result(
            &VERSIONED_NAME_UNBOUND_REG,
            type_name,
            |m: &regex::Captures| {
                // SAFETY: we know that the regex will have a match on position 0.
                let name = m.get(0).unwrap().as_str();

                // if we are misusing the function, and we cannot find the name in the hashmap,
                // we return an empty string, which will make the type tag invalid.
                if let Some(addr) = names.get(name) {
                    Ok(addr.to_string())
                } else {
                    Err(Error::MoveNameRegistry(MoveRegistryError::NameNotFound(
                        name.to_string(),
                    )))
                }
            },
        )?;

        Ok(struct_tag_str.to_string())
    }
}

/// Helper to replace all occurrences of a regex with a function that returns a string.
/// Used as a replacement of `regex`.replace_all().
/// The only difference is that this function returns a Result, so we can handle errors.
fn replace_all_result(
    re: &Regex,
    haystack: &str,
    replacement: impl Fn(&Captures) -> Result<String, Error>,
) -> Result<String, Error> {
    let mut new = String::with_capacity(haystack.len());
    let mut last_match = 0;
    for caps in re.captures_iter(haystack) {
        // SAFETY: we know that the regex will have a match on position 0.
        let m = caps.get(0).unwrap();
        new.push_str(&haystack[last_match..m.start()]);
        new.push_str(&replacement(&caps)?);
        last_match = m.end();
    }
    new.push_str(&haystack[last_match..]);
    Ok(new)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use sui_types::base_types::ObjectID;

    use super::NamedType;

    struct DemoData {
        input_type: String,
        expected_output: String,
        expected_names: Vec<String>,
    }

    #[test]
    fn parse_and_replace_type_successfully() {
        let mut demo_data = vec![];

        demo_data.push(DemoData {
            input_type: "@org/app::type::Type".to_string(),
            expected_output: format_type("0x0", "::type::Type"),
            expected_names: vec!["@org/app".to_string()],
        });

        demo_data.push(DemoData {
            input_type: "inner@org/app::type::Type".to_string(),
            expected_output: format_type("0x0", "::type::Type"),
            expected_names: vec!["inner@org/app".to_string()],
        });

        demo_data.push(DemoData {
            input_type: "@org/0xapp::type::Type".to_string(),
            expected_output: format_type("0x0", "::type::Type"),
            expected_names: vec!["@org/0xapp".to_string()],
        });

        demo_data.push(DemoData {
            input_type: "@org/app::type::Type<u64>".to_string(),
            expected_output: format_type("0x0", "::type::Type<u64>"),
            expected_names: vec!["@org/app".to_string()],
        });

        demo_data.push(DemoData {
            input_type: "@org/app::type::Type<@org/another-app::type::AnotherType, u64>"
                .to_string(),
            expected_output: format!(
                "{}<{}, u64>",
                format_type("0x0", "::type::Type"),
                format_type("0x1", "::type::AnotherType")
            ),
            expected_names: vec!["@org/app".to_string(), "@org/another-app".to_string()],
        });

        demo_data.push(DemoData {
            input_type: "@org/app::type::Type<@org/another-app::type::AnotherType<@org/even-more-nested::inner::Type>, 0x1::string::String>".to_string(),
            expected_output: format!("{}<{}<{}>, 0x1::string::String>", format_type("0x0", "::type::Type"), format_type("0x1", "::type::AnotherType"), format_type("0x2", "::inner::Type")),
            expected_names: vec![
                "@org/app".to_string(),
                "@org/another-app".to_string(),
                "@org/even-more-nested".to_string(),
            ],
        });

        for data in demo_data {
            let names = NamedType::parse_names(&data.input_type).unwrap();
            assert_eq!(names, data.expected_names);

            let mut mapping = HashMap::new();

            for (index, name) in data.expected_names.iter().enumerate() {
                mapping.insert(
                    name.clone(),
                    ObjectID::from_hex_literal(&format!("0x{}", index)).unwrap(),
                );
            }

            let replaced = NamedType::replace_names(&data.input_type, &mapping);
            assert_eq!(replaced.unwrap(), data.expected_output);
        }
    }

    #[test]
    fn parse_and_replace_type_errors() {
        let types = vec![
            "@org/--app::type::Type",
            "@org/app::type::Type<",
            "@org/app::type::Type<@org/another-app::type::AnotherType, u64",
            "app@org/v11241--type--::Type",
            "app-org::type::Type",
            "app",
            "@org/app::type::Type<@org/another-app::type@::AnotherType, u64>",
            "",
        ];

        // TODO: add snapshot tests for predictable errors.
        for t in types {
            assert!(NamedType::parse_names(t).is_err());
        }
    }

    fn format_type(address: &str, rest: &str) -> String {
        format!(
            "{}{}",
            ObjectID::from_hex_literal(address)
                .unwrap()
                .to_canonical_string(true),
            rest
        )
    }
}
