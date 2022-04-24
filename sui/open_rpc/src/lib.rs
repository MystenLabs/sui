use std::collections::BTreeMap;

use schemars::gen::{SchemaGenerator, SchemaSettings};
use schemars::schema::SchemaObject;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub struct ContentDescriptor {
    name: String,
    #[serde(skip_serializing_if = "is_default")]
    summary: String,
    #[serde(skip_serializing_if = "is_default")]
    description: String,
    #[serde(skip_serializing_if = "is_default")]
    required: bool,
    schema: SchemaObject,
    #[serde(skip_serializing_if = "is_default")]
    deprecated: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct Method {
    name: String,
    #[serde(skip_serializing_if = "is_default")]
    description: String,
    params: Vec<ContentDescriptor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ContentDescriptor>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Info {
    title: String,
    #[serde(skip_serializing_if = "is_default")]
    description: String,
    #[serde(skip_serializing_if = "is_default")]
    terms_of_service: String,
    #[serde(skip_serializing_if = "is_default")]
    contact: Contact,
    #[serde(skip_serializing_if = "is_default")]
    license: License,
    version: String,
}

fn is_default<T>(value: &T) -> bool
where
    T: Default + PartialEq,
{
    value == &T::default()
}

#[derive(Serialize, Deserialize, Default, PartialEq)]
struct Contact {
    name: String,
    url: String,
    email: String,
}
#[derive(Serialize, Deserialize, Default, PartialEq)]
struct License {
    name: String,
    url: String,
}

#[derive(Serialize, Deserialize)]
pub struct Project {
    openrpc: String,
    info: Info,
    methods: Vec<Method>,
    components: Components,
}

pub struct ProjectBuilder {
    pub proj_name: String,
    pub namespace: String,
    pub version: String,
    pub openrpc: String,
    pub schema_generator: SchemaGenerator,
    methods: Vec<Method>,
    content_descriptors: BTreeMap<String, ContentDescriptor>,
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
        }
    }

    pub fn build(mut self) -> Project {
        Project {
            openrpc: "1.2.6".to_owned(),
            info: Info {
                title: self.proj_name,
                version: self.version,
                license: License {
                    name: "Apache-2.0".to_string(),
                    url: "https://raw.githubusercontent.com/MystenLabs/sui/main/LICENSE"
                        .to_string(),
                },
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
    pub fn add_method(
        &mut self,
        name: &str,
        params: Vec<ContentDescriptor>,
        result: Option<ContentDescriptor>,
        doc: &str,
    ) {
        self.methods.push(Method {
            name: format!("{}_{}", self.namespace, name),
            description: doc.to_string(),
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
            summary: summary.to_string(),
            description: description.to_string(),
            required,
            schema,
            deprecated: false,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Components {
    content_descriptors: BTreeMap<String, ContentDescriptor>,
    schemas: BTreeMap<String, SchemaObject>,
}
