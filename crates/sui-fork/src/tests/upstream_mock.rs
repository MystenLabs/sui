// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Scripted GraphQL upstream for fault-injection tests. One wiremock catch-all responder parses
//! every request body, classifies each object key it asks for, replies from a per-`(kind, id)`
//! script (a valid object, a valid `null`, or any status and body), and counts the keys it was
//! asked for.
//!
//! Requests are classified by the variables of `object_query::MultiGetObjectsVars` and
//! `object_query::VersionAtCheckpointVars`, as cynic serializes them: camelCase names, with absent
//! fields sent as `null`.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::io;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;

use fastcrypto::encoding::Base64 as FastCryptoBase64;
use move_core_types::language_storage::TypeTag;
use serde_json::Value;
use serde_json::json;
use sui_types::base_types::ObjectID;
use sui_types::dynamic_field::derive_dynamic_field_id;
use sui_types::object::Object;
use wiremock::Match;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;

/// Requests the GraphQL client sends for one object read that fails every attempt.
pub(crate) const ATTEMPTS_PER_OBJECT_READ: usize = crate::gql::MAX_ATTEMPTS as usize;

/// Body of an HTML error page, as served by a proxy in front of the GraphQL service.
const HTML_ERROR_BODY: &str =
    "<html><head><title>502 Bad Gateway</title></head><body>502 Bad Gateway</body></html>";

/// Body the GraphQL service sends, with HTTP 500, when handling a request panics
/// (`sui-indexer-alt-graphql/src/error.rs`, `PanicHandler`).
pub(crate) const PANIC_ERRORS_BODY: &str = r#"{"data":null,"errors":[{"message":"Request panicked","extensions":{"code":"INTERNAL_SERVER_ERROR","chain":["scripted panic"]}}]}"#;

/// Body the GraphQL service sends, with HTTP 200, when a query runs past its time limit
/// (`sui-indexer-alt-graphql/src/extensions/timeout.rs`).
pub(crate) const TIMEOUT_ERRORS_BODY: &str = r#"{"data":null,"errors":[{"message":"Query timed out after 40.00s","extensions":{"code":"REQUEST_TIMEOUT"}}]}"#;

/// Body the GraphQL service sends, with HTTP 200, when a backend read behind `multiGetObjects`
/// fails (`sui-indexer-alt-graphql/src/error.rs`, `RpcError::InternalError`).
pub(crate) const BACKEND_ERRORS_BODY: &str = r#"{"data":null,"errors":[{"message":"Failed to load object","extensions":{"code":"INTERNAL_SERVER_ERROR"}}]}"#;

/// A response that carries data and also reports errors: its only `multiGetObjects` entry is
/// `null` next to the error that explains it. A failed entry must not read as absent. This is the
/// GraphQL shape for an error in a nullable list entry, not one this upstream is known to send.
pub(crate) const FAILED_ENTRY_BODY: &str = r#"{"data":{"multiGetObjects":[null]},"errors":[{"message":"Failed to load object","path":["multiGetObjects",0],"extensions":{"code":"INTERNAL_SERVER_ERROR"}}]}"#;

/// Priority of the catch-all responder; wiremock tries lower numbers first.
const CATCH_ALL_PRIORITY: u8 = u8::MAX;

/// Priority of dropped-response mocks, so they win over the catch-all while they have budget left.
const DROPPED_RESPONSE_PRIORITY: u8 = 1;

#[derive(Clone, Debug)]
pub(crate) enum UpstreamReply {
    /// Valid GraphQL response reporting the object absent (`null`).
    Null,
    /// Valid GraphQL response carrying this object (address, version, objectBcs = Base64(bcs(Object))).
    Object(Object),
    /// This HTTP status with this body verbatim, for example [`html_page`] or one of the GraphQL
    /// service's error bodies above.
    Raw { status: u16, body: &'static str },
}

/// An HTML error page with `status`, as served by a proxy in front of the GraphQL service.
pub(crate) fn html_page(status: u16) -> UpstreamReply {
    UpstreamReply::Raw {
        status,
        body: HTML_ERROR_BODY,
    }
}

/// How a single object key reads upstream. Keys of any other kind, such as exact versions, are
/// answered with `null` and not counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ReadKind {
    /// Bounded child read: `rootVersion` is non-null.
    Child,
    /// Latest read pinned at the fork checkpoint: `atCheckpoint` is non-null and `version` is null.
    Latest,
}

/// A scripted upstream: the mock server, its scripted replies, and every object key it was asked
/// for.
pub(crate) struct UpstreamScript {
    server: MockServer,
    state: Arc<Mutex<ScriptState>>,
}

#[derive(Default)]
struct ScriptState {
    replies: HashMap<(ReadKind, ObjectID), VecDeque<UpstreamReply>>,
    /// One entry per requested key of a known kind, in arrival order.
    requested: Vec<(ReadKind, ObjectID)>,
}

/// An object key of a known kind, parsed from a request's `variables.keys`.
type ParsedKey = (ReadKind, ObjectID);

/// The shape of a request, decided from its JSON variables alone.
enum ParsedRequest {
    /// `multiGetObjects(keys: $keys)` at the query root (`MultiGetObjectsQuery`, seed queries).
    /// Keys of an unknown kind are `None`.
    MultiGet(Vec<Option<ParsedKey>>),
    /// `checkpoint(sequenceNumber: $sequenceNumber) { query { multiGetObjects(keys: $keys) } }`.
    CheckpointScoped(Vec<Option<ParsedKey>>),
    Unrecognized,
}

impl UpstreamScript {
    /// Start a MockServer whose single catch-all responder classifies every POST by parsing its
    /// JSON variables; every object key carries every field, absent ones as `null`, so keys are
    /// classified by which fields are non-null. Unscripted `multiGetObjects` requests get a valid
    /// reply with one `null` per key, and checkpoint-scoped ones the equivalent under
    /// `checkpoint.query`. Anything else gets HTTP 404.
    pub(crate) async fn start() -> Self {
        let server = MockServer::start().await;
        let state = Arc::new(Mutex::new(ScriptState::default()));
        Mock::given(method("POST"))
            .respond_with(ScriptResponder {
                state: state.clone(),
            })
            .with_priority(CATCH_ALL_PRIORITY)
            .mount(&server)
            .await;
        Self { server, state }
    }

    pub(crate) fn uri(&self) -> String {
        self.server.uri()
    }

    /// Replies consumed in order by single-key requests of `kind` for `address`; when exhausted → Null.
    /// Appends to any replies still queued for the same key.
    pub(crate) fn script(&self, kind: ReadKind, address: ObjectID, replies: Vec<UpstreamReply>) {
        lock(&self.state)
            .replies
            .entry((kind, address))
            .or_default()
            .extend(replies);
    }

    /// Mount a higher-priority mock that receives the next `n` single-key requests of `kind` for
    /// `address` and closes the connection without a response, via wiremock `respond_with_err`.
    /// These requests are counted, and they do not consume scripted replies.
    pub(crate) async fn drop_responses(&self, kind: ReadKind, address: ObjectID, n: u64) {
        let state = self.state.clone();
        Mock::given(SingleKeyMatcher { kind, address })
            .respond_with_err(move |_: &Request| {
                lock(&state).requested.push((kind, address));
                io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "scripted upstream dropped the response",
                )
            })
            .with_priority(DROPPED_RESPONSE_PRIORITY)
            .up_to_n_times(n)
            .mount(&self.server)
            .await;
    }

    /// Number of requested keys of `kind` for `address`.
    pub(crate) fn count(&self, kind: ReadKind, address: ObjectID) -> usize {
        lock(&self.state)
            .requested
            .iter()
            .filter(|&&requested| requested == (kind, address))
            .count()
    }
}

/// The catch-all responder: answers from the script, or with valid `null`s, or 404.
struct ScriptResponder {
    state: Arc<Mutex<ScriptState>>,
}

impl Respond for ScriptResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        match parse_request(request) {
            ParsedRequest::MultiGet(keys) if keys.len() == 1 => {
                let Some(key) = keys[0] else {
                    return reply_template(&UpstreamReply::Null);
                };
                let mut state = lock(&self.state);
                state.requested.push(key);
                let reply = state
                    .replies
                    .get_mut(&key)
                    .and_then(VecDeque::pop_front)
                    .unwrap_or(UpstreamReply::Null);
                reply_template(&reply)
            }
            ParsedRequest::MultiGet(keys) => {
                record(&self.state, &keys);
                ResponseTemplate::new(200).set_body_json(json!({
                    "data": { "multiGetObjects": vec![Value::Null; keys.len()] }
                }))
            }
            ParsedRequest::CheckpointScoped(keys) => {
                record(&self.state, &keys);
                ResponseTemplate::new(200).set_body_json(json!({
                    "data": {
                        "checkpoint": {
                            "query": { "multiGetObjects": vec![Value::Null; keys.len()] }
                        }
                    }
                }))
            }
            ParsedRequest::Unrecognized => ResponseTemplate::new(404),
        }
    }
}

/// Admits exactly the single-key root `multiGetObjects` requests of one `(kind, address)`.
struct SingleKeyMatcher {
    kind: ReadKind,
    address: ObjectID,
}

impl Match for SingleKeyMatcher {
    fn matches(&self, request: &Request) -> bool {
        match parse_request(request) {
            ParsedRequest::MultiGet(keys) => keys == [Some((self.kind, self.address))],
            _ => false,
        }
    }
}

fn parse_request(request: &Request) -> ParsedRequest {
    let Ok(body) = serde_json::from_slice::<Value>(&request.body) else {
        return ParsedRequest::Unrecognized;
    };
    let variables = &body["variables"];
    let Some(keys) = variables["keys"].as_array() else {
        return ParsedRequest::Unrecognized;
    };
    let keys = keys.iter().map(parse_key).collect();
    if variables.get("sequenceNumber").is_some() {
        ParsedRequest::CheckpointScoped(keys)
    } else {
        ParsedRequest::MultiGet(keys)
    }
}

fn parse_key(key: &Value) -> Option<ParsedKey> {
    let address = ObjectID::from_str(key["address"].as_str()?).ok()?;
    let kind = match (
        key["rootVersion"].as_u64(),
        key["atCheckpoint"].as_u64(),
        key["version"].as_u64(),
    ) {
        (Some(_), _, _) => ReadKind::Child,
        (None, Some(_), None) => ReadKind::Latest,
        _ => return None,
    };
    Some((kind, address))
}

fn record(state: &Mutex<ScriptState>, keys: &[Option<ParsedKey>]) {
    lock(state).requested.extend(keys.iter().flatten());
}

fn reply_template(reply: &UpstreamReply) -> ResponseTemplate {
    match reply {
        UpstreamReply::Null => ResponseTemplate::new(200).set_body_json(json!({
            "data": { "multiGetObjects": [null] }
        })),
        UpstreamReply::Object(object) => ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "multiGetObjects": [{
                    "address": object.id().to_string(),
                    "version": object.version().value(),
                    "objectBcs": FastCryptoBase64::from_bytes(
                        &bcs::to_bytes(object).expect("object should serialize"),
                    )
                    .encoded(),
                }]
            }
        })),
        UpstreamReply::Raw { status, body } => {
            ResponseTemplate::new(*status).set_body_string(*body)
        }
    }
}

fn lock(state: &Mutex<ScriptState>) -> std::sync::MutexGuard<'_, ScriptState> {
    state.lock().expect("upstream script lock poisoned")
}

/// The id of the dynamic field that `bag::contains<u64>` and `dynamic_field::borrow` look up
/// under `parent` for `key`.
pub(crate) fn u64_field_id(parent: ObjectID, key: u64) -> ObjectID {
    let key_bytes = bcs::to_bytes(&key).expect("u64 should serialize");
    derive_dynamic_field_id(parent, &TypeTag::U64, &key_bytes)
        .expect("dynamic field id should derive")
}
