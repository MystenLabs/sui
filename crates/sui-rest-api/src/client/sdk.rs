// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use reqwest::header::HeaderValue;
use reqwest::StatusCode;
use reqwest::Url;
use sui_sdk_types::types::unresolved::Transaction as UnresolvedTransaction;
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
use sui_sdk_types::types::ValidatorCommittee;
use sui_sdk_types::types::Version;
use tap::Pipe;

use crate::rest::accounts::AccountOwnedObjectInfo;
use crate::rest::accounts::ListAccountOwnedObjectsQueryParameters;
use crate::rest::checkpoints::CheckpointResponse;
use crate::rest::checkpoints::ListCheckpointsQueryParameters;
use crate::rest::coins::CoinInfo;
use crate::rest::health::Threshold;
use crate::rest::info::NodeInfo;
use crate::rest::objects::DynamicFieldInfo;
use crate::rest::objects::ListDynamicFieldsQueryParameters;
use crate::rest::system::GasInfo;
use crate::rest::system::ProtocolConfigResponse;
use crate::rest::system::SystemStateSummary;
use crate::rest::system::X_SUI_MAX_SUPPORTED_PROTOCOL_VERSION;
use crate::rest::system::X_SUI_MIN_SUPPORTED_PROTOCOL_VERSION;
use crate::rest::transactions::ListTransactionsQueryParameters;
use crate::rest::transactions::ResolveTransactionQueryParameters;
use crate::rest::transactions::ResolveTransactionResponse;
use crate::rest::transactions::TransactionExecutionResponse;
use crate::rest::transactions::TransactionResponse;
use crate::rest::transactions::TransactionSimulationResponse;
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

        let request = self.inner.get(url);

        self.json(request).await
    }

    pub async fn health_check(&self, threshold_seconds: Option<u32>) -> Result<Response<()>> {
        let url = self.url().join("health")?;
        let query = Threshold { threshold_seconds };

        let response = self.inner.get(url).query(&query).send().await?;

        self.empty(response).await
    }

    pub async fn get_coin_info(&self, coin_type: &StructTag) -> Result<Response<CoinInfo>> {
        let url = self.url().join(&format!("coins/{coin_type}"))?;

        let request = self.inner.get(url);

        self.json(request).await
    }

    pub async fn list_account_objects(
        &self,
        account: Address,
        parameters: &ListAccountOwnedObjectsQueryParameters,
    ) -> Result<Response<Vec<AccountOwnedObjectInfo>>> {
        let url = self.url().join(&format!("account/{account}/objects"))?;

        let request = self.inner.get(url).query(parameters);

        self.json(request).await
    }

    pub async fn get_object(&self, object_id: ObjectId) -> Result<Response<Object>> {
        let url = self.url().join(&format!("objects/{object_id}"))?;

        let request = self.inner.get(url);

        self.bcs(request).await
    }

    pub async fn get_object_with_version(
        &self,
        object_id: ObjectId,
        version: Version,
    ) -> Result<Response<Object>> {
        let url = self
            .url()
            .join(&format!("objects/{object_id}/version/{version}"))?;

        let request = self.inner.get(url);

        self.bcs(request).await
    }

    pub async fn list_dynamic_fields(
        &self,
        object_id: ObjectId,
        parameters: &ListDynamicFieldsQueryParameters,
    ) -> Result<Response<Vec<DynamicFieldInfo>>> {
        let url = self.url().join(&format!("objects/{object_id}"))?;

        let request = self.inner.get(url).query(parameters);

        self.json(request).await
    }

    pub async fn get_gas_info(&self) -> Result<Response<GasInfo>> {
        let url = self.url().join("system/gas")?;

        let request = self.inner.get(url);

        self.json(request).await
    }

    pub async fn get_reference_gas_price(&self) -> Result<u64> {
        self.get_gas_info()
            .await
            .map(Response::into_inner)
            .map(|info| info.reference_gas_price)
    }

    pub async fn get_current_protocol_config(&self) -> Result<Response<ProtocolConfigResponse>> {
        let url = self.url().join("system/protocol")?;

        let request = self.inner.get(url);

        self.json(request).await
    }

    pub async fn get_protocol_config(
        &self,
        version: u64,
    ) -> Result<Response<ProtocolConfigResponse>> {
        let url = self.url().join(&format!("system/protocol/{version}"))?;

        let request = self.inner.get(url);

        self.json(request).await
    }

    pub async fn get_system_state_summary(&self) -> Result<Response<SystemStateSummary>> {
        let url = self.url().join("system")?;

        let request = self.inner.get(url);

        self.json(request).await
    }

    pub async fn get_current_committee(&self) -> Result<Response<ValidatorCommittee>> {
        let url = self.url().join("system/committee")?;

        let request = self.inner.get(url);

        self.bcs(request).await
    }

    pub async fn get_committee(&self, epoch: EpochId) -> Result<Response<ValidatorCommittee>> {
        let url = self.url().join(&format!("system/committee/{epoch}"))?;

        let request = self.inner.get(url);

        self.bcs(request).await
    }

    pub async fn get_checkpoint(
        &self,
        checkpoint_sequence_number: CheckpointSequenceNumber,
    ) -> Result<Response<CheckpointResponse>> {
        let url = self
            .url()
            .join(&format!("checkpoints/{checkpoint_sequence_number}"))?;

        let request = self.inner.get(url);

        self.bcs(request).await
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

        Ok(Response::new(checkpoint, parts))
    }

    pub async fn list_checkpoints(
        &self,
        parameters: &ListCheckpointsQueryParameters,
    ) -> Result<Response<Vec<SignedCheckpointSummary>>> {
        let url = self.url().join("checkpoints")?;

        let request = self.inner.get(url).query(parameters);

        self.bcs(request).await
        // self.protobuf::<crate::proto::ListCheckpointResponse>(request)
        //     .await?
        //     .try_map(|page| {
        //         page.checkpoints
        //             .into_iter()
        //             .map(TryInto::try_into)
        //             .collect()
        //     })
    }

    pub async fn get_full_checkpoint(
        &self,
        checkpoint_sequence_number: CheckpointSequenceNumber,
    ) -> Result<Response<CheckpointData>> {
        let url = self
            .url()
            .join(&format!("checkpoints/{checkpoint_sequence_number}/full"))?;

        let request = self.inner.get(url);

        self.bcs(request).await
        // self.protobuf::<crate::proto::FullCheckpoint>(request)
        //     .await?
        //     // TODO make this more efficient and convert directly into the sui-sdk-types version
        //     .try_map(|proto| {
        //         sui_types::full_checkpoint_content::CheckpointData::try_from(proto)
        //             .and_then(TryInto::try_into)
        //     })
    }

    pub async fn get_transaction(
        &self,
        transaction: &TransactionDigest,
    ) -> Result<Response<TransactionResponse>> {
        let url = self.url().join(&format!("transactions/{transaction}"))?;

        let request = self.inner.get(url);

        self.bcs(request).await
    }

    pub async fn list_transactions(
        &self,
        parameters: &ListTransactionsQueryParameters,
    ) -> Result<Response<Vec<TransactionResponse>>> {
        let url = self.url().join("transactions")?;

        let request = self.inner.get(url).query(parameters);

        self.bcs(request).await
    }

    pub async fn execute_transaction(
        &self,
        parameters: &ExecuteTransactionQueryParameters,
        transaction: &SignedTransaction,
    ) -> Result<Response<TransactionExecutionResponse>> {
        let url = self.url().join("transactions")?;

        let body = bcs::to_bytes(transaction)?;

        let request = self
            .inner
            .post(url)
            .query(parameters)
            .header(reqwest::header::CONTENT_TYPE, crate::rest::APPLICATION_BCS)
            .body(body);

        self.bcs(request).await
    }

    pub async fn simulate_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<Response<TransactionSimulationResponse>> {
        let url = self.url().join("transactions/simulate")?;

        let body = bcs::to_bytes(transaction)?;

        let request = self
            .inner
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, crate::rest::APPLICATION_BCS)
            .body(body);

        self.bcs(request).await
    }

    pub async fn resolve_transaction(
        &self,
        unresolved_transaction: &UnresolvedTransaction,
    ) -> Result<Response<ResolveTransactionResponse>> {
        let url = self.url.join("transactions/resolve")?;

        let request = self.inner.post(url).json(unresolved_transaction);

        self.bcs(request).await
    }

    pub async fn resolve_transaction_with_parameters(
        &self,
        unresolved_transaction: &UnresolvedTransaction,
        parameters: &ResolveTransactionQueryParameters,
    ) -> Result<Response<ResolveTransactionResponse>> {
        let url = self.url.join("transactions/resolve")?;

        let request = self
            .inner
            .post(url)
            .query(&parameters)
            .json(unresolved_transaction);

        self.bcs(request).await
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
        request: reqwest::RequestBuilder,
    ) -> Result<Response<T>> {
        let response = request
            .header(reqwest::header::ACCEPT, crate::rest::APPLICATION_JSON)
            .send()
            .await?;

        let (response, parts) = self.check_response(response).await?;

        let json = response.json().await?;
        Ok(Response::new(json, parts))
    }

    pub(super) async fn bcs<T: serde::de::DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<Response<T>> {
        let response = request
            .header(reqwest::header::ACCEPT, crate::rest::APPLICATION_BCS)
            .send()
            .await?;

        let (response, parts) = self.check_response(response).await?;

        let bytes = response.bytes().await?;
        match bcs::from_bytes(&bytes) {
            Ok(bcs) => Ok(Response::new(bcs, parts)),
            Err(e) => Err(Error::from_error(e).with_parts(parts)),
        }
    }

    #[allow(unused)]
    pub(super) async fn protobuf<T: prost::Message + std::default::Default>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<Response<T>> {
        let response = request
            .header(reqwest::header::ACCEPT, crate::rest::APPLICATION_PROTOBUF)
            .send()
            .await?;

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
