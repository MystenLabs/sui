// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use schemars::gen::{SchemaGenerator, SchemaSettings};
use schemars::schema::SchemaObject;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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

pub struct ProjectBuilder {
    proj_name: String,
    namespace: String,
    version: String,
    openrpc: String,
    schema_generator: SchemaGenerator,
    methods: Vec<Method>,
    content_descriptors: BTreeMap<String, ContentDescriptor>,
    license: Option<License>,
    contact: Option<Contact>,
    description: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    params: Vec<ContentDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ContentDescriptor>,
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

impl ProjectBuilder {
    pub fn new(proj_name: &str, namespace: &str) -> Self {
        let schema_generator = SchemaSettings::default()
            .with(|s| {
                s.definitions_path = "#/components/schemas/".to_string();
            })
            .into_generator();

        Self {
            proj_name: proj_name.to_string(),
            namespace: namespace.to_string(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            openrpc: "1.2.6".to_string(),
            schema_generator,
            methods: vec![],
            content_descriptors: BTreeMap::new(),
            license: None,
            contact: None,
            description: None,
        }
    }

    pub fn build(mut self) -> Project {
        Project {
            openrpc: self.openrpc,
            info: Info {
                title: self.proj_name,
                version: self.version,
                license: self.license,
                contact: self.contact,
                description: self.description,
                ..Default::default()
            },
            methods: self.methods,
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

    pub fn set_description(&mut self, description: &str) {
        self.description = Some(description.to_string());
    }

    pub fn set_contact(&mut self, name: &str, url: Option<String>, email: Option<String>) {
        self.contact = Some(Contact {
            name: name.to_string(),
            url,
            email,
        });
    }

    pub fn set_license(&mut self, license: &str, url: Option<String>) {
        self.license = Some(License {
            name: license.to_string(),
            url,
        });
    }

    pub fn add_method(
        &mut self,
        name: &str,
        params: Vec<ContentDescriptor>,
        result: Option<ContentDescriptor>,
        doc: &str,
    ) {
        self.methods.push(Method {
            name: format!("{}_{}", self.namespace, name),
            description: Some(doc.to_string()),
            params,
            result,
        })
    }

    pub fn create_content_descriptor<T: JsonSchema>(
        &mut self,
        name: &str,
        summary: &str,
        description: &str,
        required: bool,
    ) -> ContentDescriptor {
        let schema = self.schema_generator.subschema_for::<T>().into_object();
        ContentDescriptor {
            name: name.to_string(),
            summary: Some(summary.to_string()),
            description: Some(description.to_string()),
            required,
            schema,
            deprecated: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Components {
    content_descriptors: BTreeMap<String, ContentDescriptor>,
    schemas: BTreeMap<String, SchemaObject>,
}
