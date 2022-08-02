// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

extern crate core;

use std::collections::btree_map::Entry::Occupied;
use std::collections::BTreeMap;

use schemars::gen::{SchemaGenerator, SchemaSettings};
use schemars::schema::SchemaObject;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// OPEN-RPC documentation following the OpenRPC specification https://spec.open-rpc.org
/// The implementation is partial, only required fields and subset of optional fields
/// in the specification are implemented catered to Sui's need.
#[derive(Serialize, Deserialize, Clone)]
pub struct Project {
    openrpc: String,
    info: Info,
    methods: Vec<Method>,
    components: Components,
}

impl Project {
    pub fn new(
        title: &str,
        description: &str,
        contact_name: &str,
        url: &str,
        email: &str,
        license: &str,
        license_url: &str,
    ) -> Self {
        let version = env!("CARGO_PKG_VERSION").to_owned();
        let openrpc = "1.2.6".to_string();
        Self {
            openrpc,
            info: Info {
                title: title.to_string(),
                description: Some(description.to_string()),
                contact: Some(Contact {
                    name: contact_name.to_string(),
                    url: Some(url.to_string()),
                    email: Some(email.to_string()),
                }),
                license: Some(License {
                    name: license.to_string(),
                    url: Some(license_url.to_string()),
                }),
                version,
                ..Default::default()
            },
            methods: vec![],
            components: Components {
                content_descriptors: Default::default(),
                schemas: Default::default(),
            },
        }
    }

    pub fn add_module(&mut self, module: Module) {
        self.methods.extend(module.methods);

        self.methods.sort_by(|m, n| m.name.cmp(&n.name));

        self.components.schemas.extend(module.components.schemas);
        self.components
            .content_descriptors
            .extend(module.components.content_descriptors);
    }

    pub fn add_examples(&mut self, mut example_provider: BTreeMap<String, Vec<ExamplePairing>>) {
        for method in &mut self.methods {
            if let Occupied(entry) = example_provider.entry(method.name.clone()) {
                let examples = entry.remove();
                let param_names = method
                    .params
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>();

                // Make sure example's parameters are correct.
                for example in examples.iter() {
                    let example_param_names = example
                        .params
                        .iter()
                        .map(|param| param.name.clone())
                        .collect::<Vec<_>>();
                    assert_eq!(
                        param_names, example_param_names,
                        "Provided example parameters doesn't match the function parameters."
                    );
                }

                method.examples = examples
            }
        }
    }
}

pub struct Module {
    methods: Vec<Method>,
    components: Components,
}

pub struct RpcModuleDocBuilder {
    schema_generator: SchemaGenerator,
    methods: BTreeMap<String, Method>,
    content_descriptors: BTreeMap<String, ContentDescriptor>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct ContentDescriptor {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "default")]
    required: bool,
    schema: SchemaObject,
    #[serde(skip_serializing_if = "default")]
    deprecated: bool,
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Method {
    name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<Tag>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    params: Vec<ContentDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ContentDescriptor>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    examples: Vec<ExamplePairing>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct ExamplePairing {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    params: Vec<Example>,
    result: Example,
}

impl ExamplePairing {
    pub fn new(name: &str, params: Vec<(&str, Value)>, result: Value) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            summary: None,
            params: params
                .into_iter()
                .map(|(name, value)| Example {
                    name: name.to_string(),
                    summary: None,
                    description: None,
                    value,
                })
                .collect(),
            result: Example {
                name: "Result".to_string(),
                summary: None,
                description: None,
                value: result,
            },
        }
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Example {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    value: Value,
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Tag {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
struct Info {
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    terms_of_service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    contact: Option<Contact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    license: Option<License>,
    version: String,
}

fn default<T>(value: &T) -> bool
where
    T: Default + PartialEq,
{
    value == &T::default()
}

#[derive(Serialize, Deserialize, Default, Clone)]
struct Contact {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
}
#[derive(Serialize, Deserialize, Default, Clone)]
struct License {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

impl Default for RpcModuleDocBuilder {
    fn default() -> Self {
        let schema_generator = SchemaSettings::default()
            .with(|s| {
                s.definitions_path = "#/components/schemas/".to_string();
            })
            .into_generator();

        Self {
            schema_generator,
            methods: BTreeMap::new(),
            content_descriptors: BTreeMap::new(),
        }
    }
}

impl RpcModuleDocBuilder {
    pub fn build(mut self) -> Module {
        Module {
            methods: self.methods.into_values().collect(),
            components: Components {
                content_descriptors: self.content_descriptors,
                schemas: self
                    .schema_generator
                    .root_schema_for::<u8>()
                    .definitions
                    .into_iter()
                    .map(|(name, schema)| (name, schema.into_object()))
                    .collect::<BTreeMap<_, _>>(),
            },
        }
    }

    pub fn add_method(
        &mut self,
        namespace: &str,
        name: &str,
        params: Vec<ContentDescriptor>,
        result: Option<ContentDescriptor>,
        doc: &str,
        tag: Option<String>,
        is_pubsub: bool,
    ) {
        let description = if doc.trim().is_empty() {
            None
        } else {
            Some(doc.trim().to_string())
        };
        let name = format!("{}_{}", namespace, name);
        let mut tags = tag
            .map(|t| Tag {
                name: t,
                summary: None,
                description: None,
            })
            .into_iter()
            .collect::<Vec<_>>();

        if is_pubsub {
            tags.push(Tag {
                name: "Websocket".to_string(),
                summary: None,
                description: None,
            });
            tags.push(Tag {
                name: "PubSub".to_string(),
                summary: None,
                description: None,
            });
        }

        self.methods.insert(
            name.clone(),
            Method {
                name,
                description,
                params,
                result,
                tags,
                examples: Vec::new(),
            },
        );
    }

    pub fn create_content_descriptor<T: JsonSchema>(
        &mut self,
        name: &str,
        summary: Option<String>,
        description: Option<String>,
        required: bool,
    ) -> ContentDescriptor {
        let schema = self.schema_generator.subschema_for::<T>().into_object();
        ContentDescriptor {
            name: name.replace(' ', ""),
            summary,
            description,
            required,
            schema,
            deprecated: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Components {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    content_descriptors: BTreeMap<String, ContentDescriptor>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    schemas: BTreeMap<String, SchemaObject>,
}
