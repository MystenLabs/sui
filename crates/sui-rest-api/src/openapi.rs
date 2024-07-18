// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashSet,
    sync::{Arc, OnceLock},
};

use axum::{
    body::Bytes,
    extract::State,
    handler::Handler,
    http::Method,
    response::Html,
    routing::{get, MethodRouter},
    Router,
};
use openapiv3::v3_1::{
    Components, Header, Info, MediaType, OpenApi, Operation, Parameter, ParameterData, PathItem,
    Paths, ReferenceOr, RequestBody, Response, SchemaObject, Tag,
};
use schemars::{gen::SchemaGenerator, JsonSchema};
use tap::Pipe;

pub trait ApiEndpoint<S> {
    fn method(&self) -> Method;
    fn path(&self) -> &'static str;
    fn hidden(&self) -> bool {
        false
    }

    fn operation(&self, _generator: &mut SchemaGenerator) -> Operation {
        Operation::default()
    }

    fn handler(&self) -> RouteHandler<S>;
}

pub struct RouteHandler<S> {
    method: axum::http::Method,
    handler: MethodRouter<S>,
}

impl<S: Clone> RouteHandler<S> {
    pub fn new<H, T>(method: axum::http::Method, handler: H) -> Self
    where
        H: Handler<T, S>,
        T: 'static,
        S: Send + Sync + 'static,
    {
        let handler = MethodRouter::new().on(method.clone().try_into().unwrap(), handler);

        Self { method, handler }
    }

    pub fn method(&self) -> &axum::http::Method {
        &self.method
    }
}

pub struct Api<'a, S> {
    endpoints: Vec<&'a dyn ApiEndpoint<S>>,
    info: Info,
}

impl<'a, S> Api<'a, S> {
    pub fn new(info: Info) -> Self {
        Self {
            endpoints: Vec::new(),
            info,
        }
    }

    pub fn register_endpoints<I: IntoIterator<Item = &'a dyn ApiEndpoint<S>>>(
        &mut self,
        endpoints: I,
    ) {
        self.endpoints.extend(endpoints);
    }

    pub fn to_router(&self) -> axum::Router<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        let mut router = OpenApiDocument::new(self.openapi()).into_router();
        for endpoint in &self.endpoints {
            let handler = endpoint.handler();
            assert_eq!(handler.method(), endpoint.method());

            // we need to replace any path parameters wrapped in braces to be prefaced by a colon
            // until axum updates matchit: https://github.com/tokio-rs/axum/pull/2645
            let path = endpoint.path().replace('{', ":").replace('}', "");

            router = router.route(&path, handler.handler);
        }

        router
    }

    pub fn openapi(&self) -> openapiv3::versioned::OpenApi {
        self.gen_openapi(self.info.clone())
    }

    /// Internal routine for constructing the OpenAPI definition describing this
    /// API in its JSON form.
    fn gen_openapi(&self, info: Info) -> openapiv3::versioned::OpenApi {
        let mut openapi = OpenApi {
            info,
            ..Default::default()
        };

        let settings = schemars::gen::SchemaSettings::draft07().with(|s| {
            s.definitions_path = "#/components/schemas/".into();
            s.option_add_null_type = false;
        });
        let mut generator = schemars::gen::SchemaGenerator::new(settings);
        let mut tags = HashSet::new();

        let paths = openapi
            .paths
            .get_or_insert(openapiv3::v3_1::Paths::default());

        for endpoint in &self.endpoints {
            // Skip hidden endpoints
            if endpoint.hidden() {
                continue;
            }

            Self::register_endpoint(*endpoint, &mut generator, paths, &mut tags);
        }

        // Add OpenApi routes themselves
        let openapi_endpoints: [&dyn ApiEndpoint<_>; 3] =
            [&OpenApiExplorer, &OpenApiJson, &OpenApiYaml];
        for endpoint in openapi_endpoints {
            Self::register_endpoint(endpoint, &mut generator, paths, &mut tags);
        }

        let components = &mut openapi.components.get_or_insert_with(Components::default);

        // Add the schemas for which we generated references.
        let schemas = &mut components.schemas;

        generator
            .into_root_schema_for::<()>()
            .definitions
            .into_iter()
            .for_each(|(key, schema)| {
                let schema_object = SchemaObject {
                    json_schema: schema,
                    external_docs: None,
                    example: None,
                };
                schemas.insert(key, schema_object);
            });

        openapi.tags = tags
            .into_iter()
            .map(|tag| Tag {
                name: tag,
                ..Default::default()
            })
            .collect();
        // Sort the tags for stability
        openapi.tags.sort_by(|a, b| a.name.cmp(&b.name));

        openapi.servers = vec![openapiv3::v3_1::Server {
            url: "/v2".into(),
            ..Default::default()
        }];

        openapiv3::versioned::OpenApi::Version31(openapi)
    }

    fn register_endpoint<S2>(
        endpoint: &dyn ApiEndpoint<S2>,
        generator: &mut schemars::gen::SchemaGenerator,
        paths: &mut Paths,
        tags: &mut HashSet<String>,
    ) {
        let path = paths
            .paths
            .entry(endpoint.path().to_owned())
            .or_insert(ReferenceOr::Item(PathItem::default()));

        let pathitem = match path {
            openapiv3::v3_1::ReferenceOr::Item(ref mut item) => item,
            _ => panic!("reference not expected"),
        };

        let method_ref = match endpoint.method() {
            Method::DELETE => &mut pathitem.delete,
            Method::GET => &mut pathitem.get,
            Method::HEAD => &mut pathitem.head,
            Method::OPTIONS => &mut pathitem.options,
            Method::PATCH => &mut pathitem.patch,
            Method::POST => &mut pathitem.post,
            Method::PUT => &mut pathitem.put,
            Method::TRACE => &mut pathitem.trace,
            other => panic!("unexpected method `{}`", other),
        };

        let operation = endpoint.operation(generator);

        // Collect tags defined by this operation
        tags.extend(operation.tags.clone());

        method_ref.replace(operation);
    }
}

pub struct OpenApiDocument {
    openapi: openapiv3::versioned::OpenApi,
    json: OnceLock<Bytes>,
    yaml: OnceLock<Bytes>,
    ui: &'static str,
}

impl OpenApiDocument {
    pub fn new(openapi: openapiv3::versioned::OpenApi) -> Self {
        const OPENAPI_UI: &str = include_str!("../openapi/elements.html");
        // const OPENAPI_UI: &str = include_str!("../openapi/swagger.html");

        Self {
            openapi,
            json: OnceLock::new(),
            yaml: OnceLock::new(),
            ui: OPENAPI_UI,
        }
    }

    fn openapi(&self) -> &openapiv3::versioned::OpenApi {
        &self.openapi
    }

    fn json(&self) -> Bytes {
        self.json
            .get_or_init(|| {
                self.openapi()
                    .pipe(serde_json::to_string_pretty)
                    .unwrap()
                    .pipe(Bytes::from)
            })
            .clone()
    }

    fn yaml(&self) -> Bytes {
        self.yaml
            .get_or_init(|| {
                self.openapi()
                    .pipe(serde_yaml::to_string)
                    .unwrap()
                    .pipe(Bytes::from)
            })
            .clone()
    }

    fn ui(&self) -> &'static str {
        self.ui
    }

    pub fn into_router<S>(self) -> Router<S> {
        Router::new()
            .route("/openapi", get(openapi_ui))
            .route("/openapi.json", get(openapi_json))
            .route("/openapi.yaml", get(openapi_yaml))
            .with_state(Arc::new(self))
    }
}

pub struct OpenApiJson;

impl ApiEndpoint<Arc<OpenApiDocument>> for OpenApiJson {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/openapi.json"
    }

    fn operation(
        &self,
        _generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("OpenApi")
            .operation_id("openapi.json")
            .response(
                200,
                ResponseBuilder::new()
                    .content(mime::APPLICATION_JSON.as_ref(), MediaType::default())
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> RouteHandler<Arc<OpenApiDocument>> {
        RouteHandler::new(self.method(), openapi_json)
    }
}

pub struct OpenApiYaml;

impl ApiEndpoint<Arc<OpenApiDocument>> for OpenApiYaml {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/openapi.yaml"
    }

    fn operation(
        &self,
        _generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("OpenApi")
            .operation_id("openapi.yaml")
            .response(
                200,
                ResponseBuilder::new()
                    .content(mime::TEXT_PLAIN_UTF_8.as_ref(), MediaType::default())
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> RouteHandler<Arc<OpenApiDocument>> {
        RouteHandler::new(self.method(), openapi_yaml)
    }
}

pub struct OpenApiExplorer;

impl ApiEndpoint<Arc<OpenApiDocument>> for OpenApiExplorer {
    fn method(&self) -> axum::http::Method {
        axum::http::Method::GET
    }

    fn path(&self) -> &'static str {
        "/openapi"
    }

    fn operation(
        &self,
        _generator: &mut schemars::gen::SchemaGenerator,
    ) -> openapiv3::v3_1::Operation {
        OperationBuilder::new()
            .tag("OpenApi")
            .operation_id("OpenApi Explorer")
            .response(
                200,
                ResponseBuilder::new()
                    .content(mime::TEXT_HTML_UTF_8.as_ref(), MediaType::default())
                    .build(),
            )
            .build()
    }

    fn handler(&self) -> RouteHandler<Arc<OpenApiDocument>> {
        RouteHandler::new(self.method(), openapi_ui)
    }
}

async fn openapi_json(
    State(document): State<Arc<OpenApiDocument>>,
) -> impl axum::response::IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static(mime::APPLICATION_JSON.as_ref()),
        )],
        document.json(),
    )
}

async fn openapi_yaml(
    State(document): State<Arc<OpenApiDocument>>,
) -> impl axum::response::IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static(mime::TEXT_PLAIN_UTF_8.as_ref()),
        )],
        document.yaml(),
    )
}

async fn openapi_ui(State(document): State<Arc<OpenApiDocument>>) -> Html<&'static str> {
    Html(document.ui())
}

fn path_parameter<T: JsonSchema>(
    name: &str,
    generator: &mut SchemaGenerator,
) -> ReferenceOr<Parameter> {
    let schema_object = SchemaObject {
        json_schema: generator.subschema_for::<T>(),
        external_docs: None,
        example: None,
    };

    let parameter_data = ParameterData {
        name: name.into(),
        required: true,
        format: openapiv3::v3_1::ParameterSchemaOrContent::Schema(schema_object),
        description: None,
        deprecated: None,
        example: None,
        examples: Default::default(),
        explode: None,
        extensions: Default::default(),
    };

    ReferenceOr::Item(Parameter::Path {
        parameter_data,
        style: openapiv3::v3_1::PathStyle::Simple,
    })
}

fn query_parameters<T: JsonSchema>(generator: &mut SchemaGenerator) -> Vec<ReferenceOr<Parameter>> {
    let mut params = Vec::new();

    let schema = generator.root_schema_for::<T>().schema;

    let Some(object) = &schema.object else {
        return params;
    };

    for (name, schema) in &object.properties {
        let s = schema.clone().into_object();

        params.push(ReferenceOr::Item(Parameter::Query {
            parameter_data: ParameterData {
                name: name.clone(),
                description: s.metadata.as_ref().and_then(|m| m.description.clone()),
                required: object.required.contains(name),
                format: openapiv3::v3_1::ParameterSchemaOrContent::Schema(SchemaObject {
                    json_schema: s.into(),
                    example: None,
                    external_docs: None,
                }),
                extensions: Default::default(),
                deprecated: None,
                example: None,
                examples: Default::default(),
                explode: None,
            },
            allow_reserved: false,
            style: openapiv3::v3_1::QueryStyle::Form,
            allow_empty_value: None,
        }));
    }

    params
}

#[derive(Default)]
pub struct OperationBuilder {
    inner: Operation,
}

impl OperationBuilder {
    pub fn new() -> Self {
        Self {
            inner: Default::default(),
        }
    }

    pub fn build(&mut self) -> Operation {
        self.inner.clone()
    }

    pub fn tag<T: Into<String>>(&mut self, tag: T) -> &mut Self {
        self.inner.tags.push(tag.into());
        self
    }

    pub fn summary<T: Into<String>>(&mut self, summary: T) -> &mut Self {
        self.inner.summary = Some(summary.into());
        self
    }

    pub fn description<T: Into<String>>(&mut self, description: T) -> &mut Self {
        self.inner.description = Some(description.into());
        self
    }

    pub fn operation_id<T: Into<String>>(&mut self, operation_id: T) -> &mut Self {
        self.inner.operation_id = Some(operation_id.into());
        self
    }

    pub fn path_parameter<T: JsonSchema>(
        &mut self,
        name: &str,
        generator: &mut SchemaGenerator,
    ) -> &mut Self {
        self.inner
            .parameters
            .push(path_parameter::<T>(name, generator));
        self
    }

    pub fn query_parameters<T: JsonSchema>(
        &mut self,
        generator: &mut SchemaGenerator,
    ) -> &mut Self {
        self.inner
            .parameters
            .extend(query_parameters::<T>(generator));
        self
    }

    pub fn response(&mut self, status_code: u16, response: Response) -> &mut Self {
        let responses = self.inner.responses.get_or_insert(Default::default());
        responses.responses.insert(
            openapiv3::v3_1::StatusCode::Code(status_code),
            ReferenceOr::Item(response),
        );

        self
    }
}

#[derive(Default)]
pub struct ResponseBuilder {
    inner: Response,
}

impl ResponseBuilder {
    pub fn new() -> Self {
        Self {
            inner: Default::default(),
        }
    }

    pub fn build(&mut self) -> Response {
        self.inner.clone()
    }

    pub fn header<T: JsonSchema>(
        &mut self,
        name: &str,
        generator: &mut SchemaGenerator,
    ) -> &mut Self {
        let schema_object = SchemaObject {
            json_schema: generator.subschema_for::<T>(),
            external_docs: None,
            example: None,
        };

        let header = ReferenceOr::Item(Header {
            description: None,
            style: Default::default(),
            required: false,
            deprecated: None,
            format: openapiv3::v3_1::ParameterSchemaOrContent::Schema(schema_object),
            example: None,
            examples: Default::default(),
            extensions: Default::default(),
        });

        self.inner.headers.insert(name.into(), header);
        self
    }

    pub fn content<T: Into<String>>(
        &mut self,
        content_type: T,
        media_type: MediaType,
    ) -> &mut Self {
        self.inner.content.insert(content_type.into(), media_type);
        self
    }

    pub fn json_content<T: JsonSchema>(&mut self, generator: &mut SchemaGenerator) -> &mut Self {
        let schema_object = SchemaObject {
            json_schema: generator.subschema_for::<T>(),
            external_docs: None,
            example: None,
        };
        let media_type = MediaType {
            schema: Some(schema_object),
            ..Default::default()
        };

        self.content(mime::APPLICATION_JSON.as_ref(), media_type)
    }

    pub fn bcs_content(&mut self) -> &mut Self {
        self.content(crate::APPLICATION_BCS, MediaType::default())
    }
}

#[derive(Default)]
pub struct RequestBodyBuilder {
    inner: RequestBody,
}

impl RequestBodyBuilder {
    pub fn new() -> Self {
        Self {
            inner: Default::default(),
        }
    }

    pub fn build(&mut self) -> RequestBody {
        self.inner.clone()
    }

    pub fn content<T: Into<String>>(
        &mut self,
        content_type: T,
        media_type: MediaType,
    ) -> &mut Self {
        self.inner.content.insert(content_type.into(), media_type);
        self
    }

    pub fn json_content<T: JsonSchema>(&mut self, generator: &mut SchemaGenerator) -> &mut Self {
        let schema_object = SchemaObject {
            json_schema: generator.subschema_for::<T>(),
            external_docs: None,
            example: None,
        };
        let media_type = MediaType {
            schema: Some(schema_object),
            ..Default::default()
        };

        self.content(mime::APPLICATION_JSON.as_ref(), media_type)
    }

    pub fn bcs_content(&mut self) -> &mut Self {
        self.content(crate::APPLICATION_BCS, MediaType::default())
    }
}
