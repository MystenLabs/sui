// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use reqwest::header::HeaderValue;
use reqwest::StatusCode;
use reqwest::Url;
use sui_sdk_types::types::Address;
use sui_sdk_types::types::CheckpointData;
use sui_sdk_types::types::CheckpointDigest;
use sui_sdk_types::types::CheckpointSequenceNumber;
use sui_sdk_types::types::EpochId;
use sui_sdk_types::types::Object;
use sui_sdk_types::types::ObjectId;
use sui_sdk_types::types::SignedCheckpointSummary;
use sui_sdk_types::types::SignedTransaction;
use sui_sdk_types::types::StructTag;
use sui_sdk_types::types::Transaction;
use sui_sdk_types::types::TransactionDigest;
use sui_sdk_types::types::UnresolvedTransaction;
use sui_sdk_types::types::ValidatorCommittee;
use sui_sdk_types::types::Version;
use tap::Pipe;

use crate::accounts::AccountOwnedObjectInfo;
use crate::accounts::ListAccountOwnedObjectsQueryParameters;
use crate::checkpoints::CheckpointResponse;
use crate::checkpoints::ListCheckpointsQueryParameters;
use crate::coins::CoinInfo;
use crate::health::Threshold;
use crate::info::NodeInfo;
use crate::objects::DynamicFieldInfo;
use crate::objects::ListDynamicFieldsQueryParameters;
use crate::system::GasInfo;
use crate::system::ProtocolConfigResponse;
use crate::system::SystemStateSummary;
use crate::system::X_SUI_MAX_SUPPORTED_PROTOCOL_VERSION;
use crate::system::X_SUI_MIN_SUPPORTED_PROTOCOL_VERSION;
use crate::transactions::ListTransactionsQueryParameters;
use crate::transactions::ResolveTransactionQueryParameters;
use crate::transactions::ResolveTransactionResponse;
use crate::transactions::TransactionExecutionResponse;
use crate::transactions::TransactionResponse;
use crate::transactions::TransactionSimulationResponse;
use crate::types::X_SUI_CHAIN;
use crate::types::X_SUI_CHAIN_ID;
use crate::types::X_SUI_CHECKPOINT_HEIGHT;
use crate::types::X_SUI_CURSOR;
use crate::types::X_SUI_EPOCH;
use crate::types::X_SUI_LOWEST_AVAILABLE_CHECKPOINT;
use crate::types::X_SUI_LOWEST_AVAILABLE_CHECKPOINT_OBJECTS;
use crate::types::X_SUI_TIMESTAMP_MS;
use crate::ExecuteTransactionQueryParameters;

static USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[derive(Clone, Debug)]
pub struct Client {
    inner: reqwest::Client,
    url: Box<reqwest::Url>, // Boxed to save space
}

impl Client {
    pub fn new(url: &str) -> Result<Self> {
        let mut url = Url::parse(url).map_err(Error::from_error)?;

        if url.cannot_be_a_base() {
            return Err(Error::new_message(format!(
                "provided url '{url}' cannot be used as a base"
            )));
        }

        url.set_path("/v2/");

        let inner = reqwest::ClientBuilder::new()
            .user_agent(USER_AGENT)
            .http2_prior_knowledge()
            .build()?;

        Self {
            inner,
            url: Box::new(url),
        }
        .pipe(Ok)
    }

    pub(super) fn client(&self) -> &reqwest::Client {
        &self.inner
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    pub async fn node_info(&self) -> Result<Response<NodeInfo>> {
        let url = self.url().join("")?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_JSON)
            .send()
            .await?;

        self.json(response).await
    }

    pub async fn health_check(&self, threshold_seconds: Option<u32>) -> Result<Response<()>> {
        let url = self.url().join("health")?;
        let query = Threshold { threshold_seconds };

        let response = self.inner.get(url).query(&query).send().await?;

        self.empty(response).await
    }

    pub async fn get_coin_info(&self, coin_type: &StructTag) -> Result<Response<CoinInfo>> {
        let url = self.url().join(&format!("coins/{coin_type}"))?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_JSON)
            .send()
            .await?;

        self.json(response).await
    }

    pub async fn list_account_objects(
        &self,
        account: Address,
        parameters: &ListAccountOwnedObjectsQueryParameters,
    ) -> Result<Response<Vec<AccountOwnedObjectInfo>>> {
        let url = self.url().join(&format!("account/{account}/objects"))?;

        let response = self
            .inner
            .get(url)
            .query(parameters)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_JSON)
            .send()
            .await?;

        self.json(response).await
    }

    pub async fn get_object(&self, object_id: ObjectId) -> Result<Response<Object>> {
        let url = self.url().join(&format!("objects/{object_id}"))?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn get_object_with_version(
        &self,
        object_id: ObjectId,
        version: Version,
    ) -> Result<Response<Object>> {
        let url = self
            .url()
            .join(&format!("objects/{object_id}/version/{version}"))?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn list_dynamic_fields(
        &self,
        object_id: ObjectId,
        parameters: &ListDynamicFieldsQueryParameters,
    ) -> Result<Response<Vec<DynamicFieldInfo>>> {
        let url = self.url().join(&format!("objects/{object_id}"))?;

        let response = self
            .inner
            .get(url)
            .query(parameters)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_JSON)
            .send()
            .await?;

        self.json(response).await
    }

    pub async fn get_gas_info(&self) -> Result<Response<GasInfo>> {
        let url = self.url().join("system/gas")?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_JSON)
            .send()
            .await?;

        self.json(response).await
    }

    pub async fn get_reference_gas_price(&self) -> Result<u64> {
        self.get_gas_info()
            .await
            .map(Response::into_inner)
            .map(|info| info.reference_gas_price)
    }

    pub async fn get_current_protocol_config(&self) -> Result<Response<ProtocolConfigResponse>> {
        let url = self.url().join("system/protocol")?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_JSON)
            .send()
            .await?;

        self.json(response).await
    }

    pub async fn get_protocol_config(
        &self,
        version: u64,
    ) -> Result<Response<ProtocolConfigResponse>> {
        let url = self.url().join(&format!("system/protocol/{version}"))?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_JSON)
            .send()
            .await?;

        self.json(response).await
    }

    pub async fn get_system_state_summary(&self) -> Result<Response<SystemStateSummary>> {
        let url = self.url().join("system")?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_JSON)
            .send()
            .await?;

        self.json(response).await
    }

    pub async fn get_current_committee(&self) -> Result<Response<ValidatorCommittee>> {
        let url = self.url().join("system/committee")?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn get_committee(&self, epoch: EpochId) -> Result<Response<ValidatorCommittee>> {
        let url = self.url().join(&format!("system/committee/{epoch}"))?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn get_checkpoint(
        &self,
        checkpoint_sequence_number: CheckpointSequenceNumber,
    ) -> Result<Response<CheckpointResponse>> {
        let url = self
            .url()
            .join(&format!("checkpoints/{checkpoint_sequence_number}"))?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn get_latest_checkpoint(&self) -> Result<Response<SignedCheckpointSummary>> {
        let parameters = ListCheckpointsQueryParameters {
            limit: Some(1),
            start: None,
            direction: None,
            contents: false,
        };

        let (mut page, parts) = self.list_checkpoints(&parameters).await?.into_parts();

        let checkpoint = page
            .pop()
            .ok_or_else(|| Error::new_message("server returned empty checkpoint list"))?;
        let checkpoint = SignedCheckpointSummary {
            checkpoint: checkpoint.summary,
            signature: checkpoint.signature,
        };

        Ok(Response::new(checkpoint, parts))
    }

    pub async fn list_checkpoints(
        &self,
        parameters: &ListCheckpointsQueryParameters,
    ) -> Result<Response<Vec<CheckpointResponse>>> {
        let url = self.url().join("checkpoints")?;

        let response = self
            .inner
            .get(url)
            .query(parameters)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.protobuf::<crate::proto::CheckpointPage>(response)
            .await?
            .try_map(|page| {
                page.checkpoints
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect()
            })
    }

    pub async fn get_full_checkpoint(
        &self,
        checkpoint_sequence_number: CheckpointSequenceNumber,
    ) -> Result<Response<CheckpointData>> {
        let url = self
            .url()
            .join(&format!("checkpoints/{checkpoint_sequence_number}/full"))?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn get_transaction(
        &self,
        transaction: &TransactionDigest,
    ) -> Result<Response<TransactionResponse>> {
        let url = self.url().join(&format!("transactions/{transaction}"))?;

        let response = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn list_transactions(
        &self,
        parameters: &ListTransactionsQueryParameters,
    ) -> Result<Response<Vec<TransactionResponse>>> {
        let url = self.url().join("transactions")?;

        let response = self
            .inner
            .get(url)
            .query(parameters)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn execute_transaction(
        &self,
        parameters: &ExecuteTransactionQueryParameters,
        transaction: &SignedTransaction,
    ) -> Result<Response<TransactionExecutionResponse>> {
        let url = self.url().join("transactions")?;

        let body = bcs::to_bytes(transaction)?;

        let response = self
            .inner
            .post(url)
            .query(parameters)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .header(reqwest::header::CONTENT_TYPE, crate::APPLICATION_BCS)
            .body(body)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn simulate_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<Response<TransactionSimulationResponse>> {
        let url = self.url().join("transactions/simulate")?;

        let body = bcs::to_bytes(transaction)?;

        let response = self
            .inner
            .post(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .header(reqwest::header::CONTENT_TYPE, crate::APPLICATION_BCS)
            .body(body)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn resolve_transaction(
        &self,
        unresolved_transaction: &UnresolvedTransaction,
    ) -> Result<Response<ResolveTransactionResponse>> {
        let url = self.url.join("transactions/resolve")?;

        let response = self
            .inner
            .post(url)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .json(unresolved_transaction)
            .send()
            .await?;

        self.bcs(response).await
    }

    pub async fn resolve_transaction_with_parameters(
        &self,
        unresolved_transaction: &UnresolvedTransaction,
        parameters: &ResolveTransactionQueryParameters,
    ) -> Result<Response<ResolveTransactionResponse>> {
        let url = self.url.join("transactions/resolve")?;

        let response = self
            .inner
            .post(url)
            .query(&parameters)
            .header(reqwest::header::ACCEPT, crate::APPLICATION_BCS)
            .json(unresolved_transaction)
            .send()
            .await?;

        self.bcs(response).await
    }

    async fn check_response(
        &self,
        response: reqwest::Response,
    ) -> Result<(reqwest::Response, ResponseParts)> {
        let parts = ResponseParts::from_reqwest_response(&response);

        if !response.status().is_success() {
            let error = match response.text().await {
                Ok(body) => Error::new_message(body),
                Err(e) => Error::from_error(e),
            }
            .pipe(|e| e.with_parts(parts));

            return Err(error);
        }

        Ok((response, parts))
    }

    async fn empty(&self, response: reqwest::Response) -> Result<Response<()>> {
        let (_response, parts) = self.check_response(response).await?;
        Ok(Response::new((), parts))
    }

    async fn json<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<Response<T>> {
        let (response, parts) = self.check_response(response).await?;

        let json = response.json().await?;
        Ok(Response::new(json, parts))
    }

    pub(super) async fn bcs<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<Response<T>> {
        let (response, parts) = self.check_response(response).await?;

        let bytes = response.bytes().await?;
        match bcs::from_bytes(&bytes) {
            Ok(bcs) => Ok(Response::new(bcs, parts)),
            Err(e) => Err(Error::from_error(e).with_parts(parts)),
        }
    }

    async fn protobuf<T: prost::Message + std::default::Default>(
        &self,
        response: reqwest::Response,
    ) -> Result<Response<T>> {
        let (response, parts) = self.check_response(response).await?;

        let bytes = response.bytes().await?;
        match T::decode(bytes) {
            Ok(v) => Ok(Response::new(v, parts)),
            Err(e) => Err(Error::from_error(e).with_parts(parts)),
        }
    }
}

#[derive(Debug)]
pub struct ResponseParts {
    pub status: StatusCode,
    pub chain_id: Option<CheckpointDigest>,
    pub chain: Option<String>,
    pub epoch: Option<EpochId>,
    pub checkpoint_height: Option<CheckpointSequenceNumber>,
    pub timestamp_ms: Option<u64>,
    pub lowest_available_checkpoint: Option<CheckpointSequenceNumber>,
    pub lowest_available_checkpoint_objects: Option<CheckpointSequenceNumber>,
    pub cursor: Option<String>,
    pub min_supported_protocol_version: Option<u64>,
    pub max_supported_protocol_version: Option<u64>,
}

impl ResponseParts {
    fn from_reqwest_response(response: &reqwest::Response) -> Self {
        let headers = response.headers();
        let status = response.status();
        let chain_id = headers
            .get(X_SUI_CHAIN_ID)
            .map(HeaderValue::as_bytes)
            .and_then(|s| CheckpointDigest::from_base58(s).ok());
        let chain = headers
            .get(X_SUI_CHAIN)
            .and_then(|h| h.to_str().ok())
            .map(ToOwned::to_owned);
        let epoch = headers
            .get(X_SUI_EPOCH)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok());
        let checkpoint_height = headers
            .get(X_SUI_CHECKPOINT_HEIGHT)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok());
        let timestamp_ms = headers
            .get(X_SUI_TIMESTAMP_MS)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok());
        let lowest_available_checkpoint = headers
            .get(X_SUI_LOWEST_AVAILABLE_CHECKPOINT)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok());
        let lowest_available_checkpoint_objects = headers
            .get(X_SUI_LOWEST_AVAILABLE_CHECKPOINT_OBJECTS)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok());
        let cursor = headers
            .get(X_SUI_CURSOR)
            .and_then(|h| h.to_str().ok())
            .map(ToOwned::to_owned);
        let min_supported_protocol_version = headers
            .get(X_SUI_MIN_SUPPORTED_PROTOCOL_VERSION)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok());
        let max_supported_protocol_version = headers
            .get(X_SUI_MAX_SUPPORTED_PROTOCOL_VERSION)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse().ok());

        Self {
            status,
            chain_id,
            chain,
            epoch,
            checkpoint_height,
            timestamp_ms,
            lowest_available_checkpoint,
            lowest_available_checkpoint_objects,
            cursor,
            min_supported_protocol_version,
            max_supported_protocol_version,
        }
    }
}

#[derive(Debug)]
pub struct Response<T> {
    inner: T,

    parts: ResponseParts,
}

impl<T> Response<T> {
    pub fn new(inner: T, parts: ResponseParts) -> Self {
        Self { inner, parts }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn parts(&self) -> &ResponseParts {
        &self.parts
    }

    pub fn into_parts(self) -> (T, ResponseParts) {
        (self.inner, self.parts)
    }

    pub fn map<U, F>(self, f: F) -> Response<U>
    where
        F: FnOnce(T) -> U,
    {
        let (inner, parts) = self.into_parts();
        Response::new(f(inner), parts)
    }

    pub fn try_map<U, F, E>(self, f: F) -> Result<Response<U>>
    where
        F: FnOnce(T) -> Result<U, E>,
        E: Into<BoxError>,
    {
        let (inner, parts) = self.into_parts();
        match f(inner) {
            Ok(out) => Ok(Response::new(out, parts)),
            Err(e) => Err(Error::from_error(e).with_parts(parts)),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug)]
pub struct Error {
    inner: Box<InnerError>,
}

#[derive(Debug)]
struct InnerError {
    parts: Option<ResponseParts>,
    message: Option<String>,
    source: Option<BoxError>,
}

impl Error {
    fn empty() -> Self {
        Self {
            inner: Box::new(InnerError {
                parts: None,
                message: None,
                source: None,
            }),
        }
    }

    fn from_error<E: Into<BoxError>>(error: E) -> Self {
        Self::empty().with_error(error.into())
    }

    fn new_message<M: Into<String>>(message: M) -> Self {
        Self::empty().with_message(message.into())
    }

    fn with_parts(mut self, parts: ResponseParts) -> Self {
        self.inner.parts.replace(parts);
        self
    }

    fn with_message(mut self, message: String) -> Self {
        self.inner.message.replace(message);
        self
    }

    fn with_error(mut self, error: BoxError) -> Self {
        self.inner.source.replace(error);
        self
    }

    pub fn status(&self) -> Option<StatusCode> {
        self.parts().map(|parts| parts.status)
    }

    pub fn parts(&self) -> Option<&ResponseParts> {
        self.inner.parts.as_ref()
    }

    pub fn message(&self) -> Option<&str> {
        self.inner.message.as_deref()
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rest Client Error:")?;
        if let Some(status) = self.status() {
            write!(f, " {status}")?;
        }

        if let Some(message) = self.message() {
            write!(f, " '{message}'")?;
        }

        if let Some(source) = &self.inner.source {
            write!(f, " '{source}'")?;
        }

        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.source.as_deref().map(|e| e as _)
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Self::from_error(error)
    }
}

impl From<bcs::Error> for Error {
    fn from(error: bcs::Error) -> Self {
        Self::from_error(error)
    }
}

impl From<url::ParseError> for Error {
    fn from(error: url::ParseError) -> Self {
        Self::from_error(error)
    }
}

impl From<sui_types::sui_sdk_types_conversions::SdkTypeConversionError> for Error {
    fn from(value: sui_types::sui_sdk_types_conversions::SdkTypeConversionError) -> Self {
        Self::from_error(value)
    }
}
