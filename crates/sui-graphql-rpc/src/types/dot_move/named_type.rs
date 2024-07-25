// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use async_graphql::Context;
use futures::future;
use move_core_types::parser::parse_type_tag;
use sui_types::base_types::ObjectID;

use crate::error::Error;

use super::{
    config::{DotMoveServiceError, VERSIONED_NAME_UNBOUND_REG},
    named_move_package::NamedMovePackage,
};

pub(crate) struct NamedType;

impl NamedType {
    pub(crate) async fn query(
        ctx: &Context<'_>,
        name: &str,
        checkpoint_viewed_at: u64,
    ) -> Result<String, Error> {
        // we do not de-duplicate the names here, as the dataloader will do this for us.
        let names = Self::parse_names(name)?;

        // Gather all the requests to resolve the names.
        let names_to_resolve = names
            .iter()
            .map(|x| NamedMovePackage::query(ctx, x, checkpoint_viewed_at))
            .collect::<Vec<_>>();

        // now we resolve all the names in parallel (data-loader will do the proper de-duplication / batching for us)
        // also the `NamedMovePackage` query will re-validate the names (including max length, which is not checked on the regex).
        let mut results = future::try_join_all(names_to_resolve).await?;

        // now let's create a hashmap with {name: MovePackage}
        let mut name_package_id_mapping = HashMap::new();

        // doing it in reverse so we can pop instead of shift
        for name in names.into_iter().rev() {
            // safe unwrap: we know that the amount of results has to equal the amount of names.
            let Some(package) = results.pop().unwrap() else {
                return Err(Error::DotMove(DotMoveServiceError::NameNotFound(name)));
            };

            name_package_id_mapping.insert(name, package.native.id());
        }

        let correct_type_tag = Self::replace_names(name, &name_package_id_mapping);

        // now we query the names with futures to utilize data loader
        Ok(correct_type_tag)
    }

    // TODO: Should we introduce some overall string limit length here?
    // Is this already caught by the global limits?
    // This parser just extracts all names from a type tag, and returns them
    // We do not care about de-duplication, as the dataloader will do this for us.
    // The goal of replacing all of them with `0x0` is to make sure that the type tag is valid
    // so when replaced with the move name package addresses, it'll also be valid.
    fn parse_names(name: &str) -> Result<Vec<String>, Error> {
        let mut names = vec![];
        let struct_tag = VERSIONED_NAME_UNBOUND_REG.replace_all(name, |m: &regex::Captures| {
            // safe unwrap: we know that the regex will always have a match on position 0.
            names.push(m.get(0).unwrap().as_str().to_string());
            "0x0".to_string()
        });

        // We attempt to parse the type_tag with these replacements, to make sure there are no other
        // errors in the type tag (apart from the move names). That protects us from unnecessary
        // queries to resolve .move names, for a type tag that will be invalid anyway.
        parse_type_tag(&struct_tag).map_err(|e| Error::Client(e.to_string()))?;

        Ok(names)
    }

    // This function replaces all the names in the type tag with their corresponding MovePackage address.
    // The names are guaranteed to be the same and exist (as long as this is called in sequence),
    // since we use the same parser to extract the names.
    fn replace_names(type_name: &str, names: &HashMap<String, ObjectID>) -> String {
        let struct_tag_str =
            VERSIONED_NAME_UNBOUND_REG.replace_all(type_name, |m: &regex::Captures| {
                // safe unwrap: we know that the regex will have a match on position 0.
                let name = m.get(0).unwrap().as_str();

                // if we are miss-using the function, and we cannot find the name in the hashmap,
                // we return an empty string, which will make the type tag invalid.
                if let Some(addr) = names.get(name) {
                    addr.to_string()
                } else {
                    "".to_string()
                }
            });

        struct_tag_str.to_string()
    }
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
            input_type: "app@org::type::Type".to_string(),
            expected_output: format_type("0x0", "::type::Type"),
            expected_names: vec!["app@org".to_string()],
        });

        demo_data.push(DemoData {
            input_type: "0xapp@org::type::Type".to_string(),
            expected_output: format_type("0x0", "::type::Type"),
            expected_names: vec!["0xapp@org".to_string()],
        });

        demo_data.push(DemoData {
            input_type: "app@org::type::Type<u64>".to_string(),
            expected_output: format!("{}<u64>", format_type("0x0", "::type::Type")),
            expected_names: vec!["app@org".to_string()],
        });

        demo_data.push(DemoData {
            input_type: "app@org::type::Type<another-app@org::type::AnotherType, u64>".to_string(),
            expected_output: format!(
                "{}<{}, u64>",
                format_type("0x0", "::type::Type"),
                format_type("0x1", "::type::AnotherType")
            ),
            expected_names: vec!["app@org".to_string(), "another-app@org".to_string()],
        });

        demo_data.push(DemoData {
            input_type: "app@org::type::Type<another-app@org::type::AnotherType<even-more-nested@org::inner::Type>, 0x1::string::String>".to_string(),
            expected_output: format!("{}<{}<{}>, 0x1::string::String>", format_type("0x0", "::type::Type"), format_type("0x1", "::type::AnotherType"), format_type("0x2", "::inner::Type")),
            expected_names: vec![
                "app@org".to_string(),
                "another-app@org".to_string(),
                "even-more-nested@org".to_string(),
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
            assert_eq!(replaced, data.expected_output);
        }
    }

    #[test]
    fn parse_and_replace_type_errors() {
        let types = vec![
            "--app@org::type::Type",
            "app@org::type::Type<",
            "app@org::type::Type<another-app@org::type::AnotherType, u64",
            "app@org/v11241--type::Type",
            "app--org::type::Type",
            "app",
            "app@org::type::Type<another-app@org::type@::AnotherType, u64>",
            "",
        ];

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
