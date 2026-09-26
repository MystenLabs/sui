// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! How `ForkStore` reads handle upstream GraphQL responses that are not an answer: an HTTP error
//! status, a body that is not a GraphQL response, or a GraphQL response that reports errors. None
//! of them reads as "not found": the read fails with an error saying the upstream GraphQL request
//! failed, and why. An object read first sends the request again after any failure, a bounded
//! number of times. Through a storage trait that cannot return the error, a failed object read
//! panics. Exercised through the bounded child read, the read the Move VM makes for a dynamic
//! field, a latest object read and a checkpoint lookup, against local upstreams.

use std::panic::AssertUnwindSafe;

use sui_types::digests::TransactionDigest;
use sui_types::object::Data;
use sui_types::object::MoveObject;
use sui_types::object::ObjectInner;
use sui_types::object::Owner;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;

use super::*;
use crate::services::ServiceManager;
use crate::upstream_mock::ATTEMPTS_PER_OBJECT_READ;
use crate::upstream_mock::BACKEND_ERRORS_BODY;
use crate::upstream_mock::FAILED_ENTRY_BODY;
use crate::upstream_mock::PANIC_ERRORS_BODY;
use crate::upstream_mock::ReadKind;
use crate::upstream_mock::TIMEOUT_ERRORS_BODY;
use crate::upstream_mock::UpstreamReply;
use crate::upstream_mock::UpstreamScript;
use crate::upstream_mock::html_page;

const FORKED_AT_CHECKPOINT: CheckpointSequenceNumber = 42;

/// Root-version bound passed to every child read.
const CHILD_BOUND: SequenceNumber = SequenceNumber::from_u64(10);

/// Every error for a failed upstream request starts with this.
const FAILED_REQUEST: &str = "upstream GraphQL request failed";

/// A fork store over a fresh tempdir and data services, pointed at `upstream_url`. Fields drop in
/// declaration order, so the store closes before its services and tempdir.
struct Store {
    store: ForkStore,
    _services: ServiceManager,
    _temp: tempfile::TempDir,
}

impl Store {
    fn new(upstream_url: String) -> Self {
        let temp = tempfile::tempdir().expect("failed to create tempdir");
        let services = ServiceManager::open(
            temp.path(),
            "custom".to_owned(),
            FORKED_AT_CHECKPOINT,
            CheckpointDigest::new([9; 32]).into(),
        )
        .expect("service manager should open");
        let store = ForkStore::new_for_testing_with_remote(
            temp.path().to_path_buf(),
            upstream_url,
            FORKED_AT_CHECKPOINT,
            services.local_store(),
        );
        Self {
            store,
            _services: services,
            _temp: temp,
        }
    }

    /// A store whose upstream answers every request with `status` and `body`.
    async fn answering(status: u16, body: &'static str) -> (Self, MockServer) {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .mount(&server)
            .await;
        (Self::new(server.uri()), server)
    }
}

/// A store over a scripted upstream, plus the parent and child the reads use.
struct Fixture {
    store: Store,
    script: UpstreamScript,
    parent: ObjectID,
    child: ObjectID,
}

impl Fixture {
    async fn new() -> Self {
        let script = UpstreamScript::start().await;
        Self {
            store: Store::new(script.uri()),
            script,
            parent: ObjectID::random(),
            child: ObjectID::random(),
        }
    }

    /// Script the upstream's replies to this fixture's child reads, in request order.
    fn script_child(&self, replies: Vec<UpstreamReply>) {
        self.script.script(ReadKind::Child, self.child, replies);
    }

    /// The child read the Move VM makes, which panics if the read fails.
    fn read_child(&self) -> SuiResult<Option<Object>> {
        self.store
            .store
            .read_child_object(&self.parent, &self.child, CHILD_BOUND)
    }

    /// The error of the store's fallible read behind [`Self::read_child`], which must fail.
    fn read_child_error(&self) -> String {
        let error = self
            .store
            .store
            .get_object_lt_or_eq_version(&self.child, CHILD_BOUND)
            .expect_err("a read that got no answer must fail rather than read as absent");
        format!("{error:#}")
    }

    fn child_requests(&self) -> usize {
        self.script.count(ReadKind::Child, self.child)
    }
}

/// A child object that passes the resolver's owner check for `parent`.
fn child_object(parent: ObjectID, child: ObjectID, version: u64) -> Object {
    let move_object = MoveObject::new_gas_coin(SequenceNumber::from_u64(version), child, 1_000_000);
    ObjectInner {
        owner: Owner::ObjectOwner(parent.into()),
        data: Data::Move(move_object),
        previous_transaction: TransactionDigest::genesis_marker(),
        storage_rebate: 0,
    }
    .into()
}

/// The message `read` panics with.
fn panic_message<T>(read: impl FnOnce() -> T) -> String {
    let payload = std::panic::catch_unwind(AssertUnwindSafe(read))
        .err()
        .expect("the failed read should panic");
    *payload
        .downcast::<String>()
        .expect("the panic message should be a String")
}

#[tokio::test]
async fn any_failed_child_read_is_resent() {
    for reply in [
        html_page(502),
        html_page(404),
        html_page(200),
        UpstreamReply::Raw {
            status: 500,
            body: PANIC_ERRORS_BODY,
        },
        UpstreamReply::Raw {
            status: 200,
            body: TIMEOUT_ERRORS_BODY,
        },
        UpstreamReply::Raw {
            status: 200,
            body: BACKEND_ERRORS_BODY,
        },
        UpstreamReply::Raw {
            status: 200,
            body: FAILED_ENTRY_BODY,
        },
        UpstreamReply::Raw {
            status: 200,
            body: r#"{"message":"Internal server error"}"#,
        },
        UpstreamReply::Raw {
            status: 200,
            body: "",
        },
    ] {
        let fixture = Fixture::new().await;
        fixture.script_child(vec![reply.clone(), UpstreamReply::Null]);

        let child = fixture.read_child().unwrap_or_else(|err| {
            panic!("the resend's answer should be read after {reply:?}: {err}")
        });

        assert!(child.is_none(), "{reply:?}");
        assert_eq!(fixture.child_requests(), 2, "{reply:?}");
    }
}

#[tokio::test]
async fn child_read_resends_until_the_child_arrives() {
    let fixture = Fixture::new().await;
    let expected = child_object(fixture.parent, fixture.child, 5);
    fixture.script_child(vec![
        html_page(502),
        UpstreamReply::Object(expected.clone()),
    ]);

    let child = fixture
        .read_child()
        .expect("the read should succeed once an answer arrives")
        .expect("the upstream's child should be returned");

    assert_eq!(
        child.compute_object_reference(),
        expected.compute_object_reference()
    );
    assert_eq!(fixture.child_requests(), 2);
}

#[tokio::test]
async fn a_request_whose_response_never_arrives_is_resent() {
    let fixture = Fixture::new().await;
    fixture
        .script
        .drop_responses(ReadKind::Child, fixture.child, 1)
        .await;

    let child = fixture
        .read_child()
        .expect("the read should succeed once a response arrives");

    assert!(child.is_none());
    assert_eq!(fixture.child_requests(), 2);
}

#[tokio::test]
async fn a_read_that_never_gets_a_response_fails_after_every_attempt() {
    let fixture = Fixture::new().await;
    fixture
        .script
        .drop_responses(
            ReadKind::Child,
            fixture.child,
            ATTEMPTS_PER_OBJECT_READ as u64,
        )
        .await;

    let error = fixture.read_child_error();

    assert!(
        error.starts_with(&format!(
            "{FAILED_REQUEST} after {ATTEMPTS_PER_OBJECT_READ} attempts: no response"
        )),
        "{error}"
    );
    assert_eq!(fixture.child_requests(), ATTEMPTS_PER_OBJECT_READ);
}

/// The Move VM would turn a failed child read into an invariant violation, which execute commits,
/// so the read panics instead, naming the upstream failure.
#[tokio::test]
async fn a_child_read_that_fails_every_attempt_panics() {
    let fixture = Fixture::new().await;
    fixture.script_child(vec![html_page(502); ATTEMPTS_PER_OBJECT_READ]);

    let message = panic_message(|| fixture.read_child());

    assert!(
        message.starts_with(&format!(
            "read of child object {} failed: {FAILED_REQUEST} after {ATTEMPTS_PER_OBJECT_READ} \
             attempts",
            fixture.child,
        )),
        "{message}"
    );
    assert!(message.contains("HTTP 502"), "{message}");
    assert_eq!(fixture.child_requests(), ATTEMPTS_PER_OBJECT_READ);
}

/// An object read through `ObjectStore`, which can only answer with `Option`, panics rather than
/// read a failure as a missing object, including a backend failure the upstream reports with
/// HTTP 200.
#[tokio::test]
async fn an_object_read_that_fails_every_attempt_panics_instead_of_reading_as_absent() {
    let fixture = Fixture::new().await;
    let object = ObjectID::random();
    fixture.script.script(
        ReadKind::Latest,
        object,
        vec![
            UpstreamReply::Raw {
                status: 200,
                body: BACKEND_ERRORS_BODY,
            };
            ATTEMPTS_PER_OBJECT_READ
        ],
    );

    let message = panic_message(|| ObjectStore::get_object(&fixture.store.store, &object));

    assert!(
        message.starts_with(&format!(
            "latest read of object {object} failed: {FAILED_REQUEST} after \
             {ATTEMPTS_PER_OBJECT_READ} attempts"
        )),
        "{message}"
    );
    assert!(message.contains("(INTERNAL_SERVER_ERROR)"), "{message}");
    assert_eq!(
        fixture.script.count(ReadKind::Latest, object),
        ATTEMPTS_PER_OBJECT_READ
    );
}

/// A checkpoint lookup whose response is an error status or reports GraphQL errors fails, rather
/// than read as a missing checkpoint.
#[tokio::test]
async fn a_failed_checkpoint_lookup_fails_instead_of_reading_as_absent() {
    for (status, body, detail) in [
        (500, PANIC_ERRORS_BODY, "HTTP 500"),
        (200, TIMEOUT_ERRORS_BODY, "(REQUEST_TIMEOUT)"),
    ] {
        let (store, _server) = Store::answering(status, body).await;

        let error = store
            .store
            .get_checkpoint_by_sequence_number(FORKED_AT_CHECKPOINT - 1)
            .expect_err("a response that is not an answer must not read as a missing checkpoint");

        let error = format!("{error:#}");
        assert!(error.contains(FAILED_REQUEST), "{error}");
        assert!(error.contains(detail), "{error}");
    }
}
