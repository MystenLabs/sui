// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CertificateDigest {
    #[prost(bytes="bytes", tag="1")]
    pub digest: ::prost::bytes::Bytes,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BatchDigest {
    #[prost(bytes="bytes", tag="1")]
    pub digest: ::prost::bytes::Bytes,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Batch {
    #[prost(message, repeated, tag="1")]
    pub transaction: ::prost::alloc::vec::Vec<Transaction>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Transaction {
    #[prost(bytes="bytes", tag="1")]
    pub transaction: ::prost::bytes::Bytes,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CollectionError {
    #[prost(message, optional, tag="1")]
    pub id: ::core::option::Option<CertificateDigest>,
    #[prost(enumeration="CollectionErrorType", tag="2")]
    pub error: i32,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BatchMessage {
    #[prost(message, optional, tag="1")]
    pub id: ::core::option::Option<BatchDigest>,
    #[prost(message, optional, tag="2")]
    pub transactions: ::core::option::Option<Batch>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PrimaryAddresses {
    #[prost(message, optional, tag="1")]
    pub primary_to_primary: ::core::option::Option<MultiAddr>,
    #[prost(message, optional, tag="2")]
    pub worker_to_primary: ::core::option::Option<MultiAddr>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MultiAddr {
    #[prost(string, tag="1")]
    pub address: ::prost::alloc::string::String,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PublicKey {
    #[prost(bytes="bytes", tag="1")]
    pub bytes: ::prost::bytes::Bytes,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ValidatorData {
    #[prost(message, optional, tag="1")]
    pub public_key: ::core::option::Option<PublicKey>,
    #[prost(int64, tag="2")]
    pub stake_weight: i64,
    #[prost(message, optional, tag="3")]
    pub primary_addresses: ::core::option::Option<PrimaryAddresses>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CollectionRetrievalResult {
    #[prost(oneof="collection_retrieval_result::RetrievalResult", tags="1, 2")]
    pub retrieval_result: ::core::option::Option<collection_retrieval_result::RetrievalResult>,
}
/// Nested message and enum types in `CollectionRetrievalResult`.
pub mod collection_retrieval_result {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum RetrievalResult {
        #[prost(message, tag="1")]
        Batch(super::BatchMessage),
        #[prost(message, tag="2")]
        Error(super::CollectionError),
    }
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetCollectionsRequest {
    /// List of collections to be retreived.
    #[prost(message, repeated, tag="1")]
    pub collection_ids: ::prost::alloc::vec::Vec<CertificateDigest>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GetCollectionsResponse {
    /// TODO: Revisit this for spec compliance.
    /// List of retrieval results of collections.
    #[prost(message, repeated, tag="1")]
    pub result: ::prost::alloc::vec::Vec<CollectionRetrievalResult>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RemoveCollectionsRequest {
    /// List of collections to be removed.
    #[prost(message, repeated, tag="1")]
    pub collection_ids: ::prost::alloc::vec::Vec<CertificateDigest>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ReadCausalRequest {
    /// A collection for which a sequence of related collections are to be retrieved.
    #[prost(message, optional, tag="1")]
    pub collection_id: ::core::option::Option<CertificateDigest>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ReadCausalResponse {
    /// Resulting sequence of collections from DAG walk.
    #[prost(message, repeated, tag="1")]
    pub collection_ids: ::prost::alloc::vec::Vec<CertificateDigest>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RoundsRequest {
    //// The validator's key for which we want to retrieve
    //// the available rounds.
    #[prost(message, optional, tag="1")]
    pub public_key: ::core::option::Option<PublicKey>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RoundsResponse {
    //// The oldest round for which the node has available
    //// blocks to propose for the defined validator.
    #[prost(uint64, tag="1")]
    pub oldest_round: u64,
    //// The newest (latest) round for which the node has available
    //// blocks to propose for the defined validator.
    #[prost(uint64, tag="2")]
    pub newest_round: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NodeReadCausalRequest {
    #[prost(message, optional, tag="1")]
    pub public_key: ::core::option::Option<PublicKey>,
    #[prost(uint64, tag="2")]
    pub round: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NodeReadCausalResponse {
    /// Resulting sequence of collections from DAG walk.
    #[prost(message, repeated, tag="1")]
    pub collection_ids: ::prost::alloc::vec::Vec<CertificateDigest>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewNetworkInfoRequest {
    #[prost(uint32, tag="1")]
    pub epoch_number: u32,
    #[prost(message, repeated, tag="2")]
    pub validators: ::prost::alloc::vec::Vec<ValidatorData>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewEpochRequest {
    #[prost(uint32, tag="1")]
    pub epoch_number: u32,
    #[prost(message, repeated, tag="2")]
    pub validators: ::prost::alloc::vec::Vec<ValidatorData>,
}
/// A bincode encoded payload. This is intended to be used in the short-term
/// while we don't have good protobuf definitions for Narwhal types
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BincodeEncodedPayload {
    #[prost(bytes="bytes", tag="1")]
    pub payload: ::prost::bytes::Bytes,
}
/// Empty message for when we don't have anything to return
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Empty {
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum CollectionErrorType {
    CollectionNotFound = 0,
    CollectionTimeout = 1,
    CollectionError = 2,
}
/// Generated client implementations.
pub mod validator_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// The consensus to mempool interface for validator actions.
    #[derive(Debug, Clone)]
    pub struct ValidatorClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl ValidatorClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> ValidatorClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> ValidatorClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            ValidatorClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with `gzip`.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_gzip(mut self) -> Self {
            self.inner = self.inner.send_gzip();
            self
        }
        /// Enable decompressing responses with `gzip`.
        #[must_use]
        pub fn accept_gzip(mut self) -> Self {
            self.inner = self.inner.accept_gzip();
            self
        }
        /// Returns collection contents for each requested collection.
        pub async fn get_collections(
            &mut self,
            request: impl tonic::IntoRequest<super::GetCollectionsRequest>,
        ) -> Result<tonic::Response<super::GetCollectionsResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.Validator/GetCollections",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        /// Expunges collections from the mempool.
        pub async fn remove_collections(
            &mut self,
            request: impl tonic::IntoRequest<super::RemoveCollectionsRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.Validator/RemoveCollections",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        /// Returns collections along a DAG walk with a well-defined starting point.
        pub async fn read_causal(
            &mut self,
            request: impl tonic::IntoRequest<super::ReadCausalRequest>,
        ) -> Result<tonic::Response<super::ReadCausalResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.Validator/ReadCausal",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
/// Generated client implementations.
pub mod proposer_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    //// The API that hosts the endpoints that should be used to help
    //// proposing a block.
    #[derive(Debug, Clone)]
    pub struct ProposerClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl ProposerClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> ProposerClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> ProposerClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            ProposerClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with `gzip`.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_gzip(mut self) -> Self {
            self.inner = self.inner.send_gzip();
            self
        }
        /// Enable decompressing responses with `gzip`.
        #[must_use]
        pub fn accept_gzip(mut self) -> Self {
            self.inner = self.inner.accept_gzip();
            self
        }
        pub async fn rounds(
            &mut self,
            request: impl tonic::IntoRequest<super::RoundsRequest>,
        ) -> Result<tonic::Response<super::RoundsResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/narwhal.Proposer/Rounds");
            self.inner.unary(request.into_request(), path, codec).await
        }
        /// Returns the read_causal obtained by starting the DAG walk at the collection
        /// proposed by the input authority (as indicated by their public key) at the input round
        pub async fn node_read_causal(
            &mut self,
            request: impl tonic::IntoRequest<super::NodeReadCausalRequest>,
        ) -> Result<tonic::Response<super::NodeReadCausalResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.Proposer/NodeReadCausal",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
/// Generated client implementations.
pub mod configuration_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    #[derive(Debug, Clone)]
    pub struct ConfigurationClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl ConfigurationClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> ConfigurationClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> ConfigurationClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            ConfigurationClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with `gzip`.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_gzip(mut self) -> Self {
            self.inner = self.inner.send_gzip();
            self
        }
        /// Enable decompressing responses with `gzip`.
        #[must_use]
        pub fn accept_gzip(mut self) -> Self {
            self.inner = self.inner.accept_gzip();
            self
        }
        /// Signals a new epoch
        pub async fn new_epoch(
            &mut self,
            request: impl tonic::IntoRequest<super::NewEpochRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.Configuration/NewEpoch",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        /// Signals a change in networking info
        pub async fn new_network_info(
            &mut self,
            request: impl tonic::IntoRequest<super::NewNetworkInfoRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.Configuration/NewNetworkInfo",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
/// Generated client implementations.
pub mod primary_to_primary_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// The primary-to-primary interface
    #[derive(Debug, Clone)]
    pub struct PrimaryToPrimaryClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl PrimaryToPrimaryClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> PrimaryToPrimaryClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> PrimaryToPrimaryClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            PrimaryToPrimaryClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with `gzip`.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_gzip(mut self) -> Self {
            self.inner = self.inner.send_gzip();
            self
        }
        /// Enable decompressing responses with `gzip`.
        #[must_use]
        pub fn accept_gzip(mut self) -> Self {
            self.inner = self.inner.accept_gzip();
            self
        }
        /// Sends a message
        pub async fn send_message(
            &mut self,
            request: impl tonic::IntoRequest<super::BincodeEncodedPayload>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.PrimaryToPrimary/SendMessage",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
/// Generated client implementations.
pub mod worker_to_worker_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// The worker-to-worker interface
    #[derive(Debug, Clone)]
    pub struct WorkerToWorkerClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl WorkerToWorkerClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> WorkerToWorkerClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> WorkerToWorkerClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            WorkerToWorkerClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with `gzip`.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_gzip(mut self) -> Self {
            self.inner = self.inner.send_gzip();
            self
        }
        /// Enable decompressing responses with `gzip`.
        #[must_use]
        pub fn accept_gzip(mut self) -> Self {
            self.inner = self.inner.accept_gzip();
            self
        }
        /// Sends a worker message
        pub async fn send_message(
            &mut self,
            request: impl tonic::IntoRequest<super::BincodeEncodedPayload>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.WorkerToWorker/SendMessage",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        /// requests a number of batches that the service then streams back to the client
        pub async fn client_batch_request(
            &mut self,
            request: impl tonic::IntoRequest<super::BincodeEncodedPayload>,
        ) -> Result<
            tonic::Response<tonic::codec::Streaming<super::BincodeEncodedPayload>>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.WorkerToWorker/ClientBatchRequest",
            );
            self.inner.server_streaming(request.into_request(), path, codec).await
        }
    }
}
/// Generated client implementations.
pub mod worker_to_primary_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// The worker-to-primary interface
    #[derive(Debug, Clone)]
    pub struct WorkerToPrimaryClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl WorkerToPrimaryClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> WorkerToPrimaryClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> WorkerToPrimaryClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            WorkerToPrimaryClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with `gzip`.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_gzip(mut self) -> Self {
            self.inner = self.inner.send_gzip();
            self
        }
        /// Enable decompressing responses with `gzip`.
        #[must_use]
        pub fn accept_gzip(mut self) -> Self {
            self.inner = self.inner.accept_gzip();
            self
        }
        /// Sends a message
        pub async fn send_message(
            &mut self,
            request: impl tonic::IntoRequest<super::BincodeEncodedPayload>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.WorkerToPrimary/SendMessage",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
/// Generated client implementations.
pub mod primary_to_worker_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    /// The primary-to-worker interface
    #[derive(Debug, Clone)]
    pub struct PrimaryToWorkerClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl PrimaryToWorkerClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> PrimaryToWorkerClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> PrimaryToWorkerClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            PrimaryToWorkerClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with `gzip`.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_gzip(mut self) -> Self {
            self.inner = self.inner.send_gzip();
            self
        }
        /// Enable decompressing responses with `gzip`.
        #[must_use]
        pub fn accept_gzip(mut self) -> Self {
            self.inner = self.inner.accept_gzip();
            self
        }
        /// Sends a message
        pub async fn send_message(
            &mut self,
            request: impl tonic::IntoRequest<super::BincodeEncodedPayload>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.PrimaryToWorker/SendMessage",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
    }
}
/// Generated client implementations.
pub mod transactions_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    #[derive(Debug, Clone)]
    pub struct TransactionsClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl TransactionsClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: std::convert::TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> TransactionsClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> TransactionsClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::BoxBody>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::BoxBody>,
            >>::Error: Into<StdError> + Send + Sync,
        {
            TransactionsClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with `gzip`.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_gzip(mut self) -> Self {
            self.inner = self.inner.send_gzip();
            self
        }
        /// Enable decompressing responses with `gzip`.
        #[must_use]
        pub fn accept_gzip(mut self) -> Self {
            self.inner = self.inner.accept_gzip();
            self
        }
        /// Submit a Transactions
        pub async fn submit_transaction(
            &mut self,
            request: impl tonic::IntoRequest<super::Transaction>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.Transactions/SubmitTransaction",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        /// Submit a Transactions
        pub async fn submit_transaction_stream(
            &mut self,
            request: impl tonic::IntoStreamingRequest<Message = super::Transaction>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::new(
                        tonic::Code::Unknown,
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/narwhal.Transactions/SubmitTransactionStream",
            );
            self.inner
                .client_streaming(request.into_streaming_request(), path, codec)
                .await
        }
    }
}
/// Generated server implementations.
pub mod validator_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    ///Generated trait containing gRPC methods that should be implemented for use with ValidatorServer.
    #[async_trait]
    pub trait Validator: Send + Sync + 'static {
        /// Returns collection contents for each requested collection.
        async fn get_collections(
            &self,
            request: tonic::Request<super::GetCollectionsRequest>,
        ) -> Result<tonic::Response<super::GetCollectionsResponse>, tonic::Status>;
        /// Expunges collections from the mempool.
        async fn remove_collections(
            &self,
            request: tonic::Request<super::RemoveCollectionsRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        /// Returns collections along a DAG walk with a well-defined starting point.
        async fn read_causal(
            &self,
            request: tonic::Request<super::ReadCausalRequest>,
        ) -> Result<tonic::Response<super::ReadCausalResponse>, tonic::Status>;
    }
    /// The consensus to mempool interface for validator actions.
    #[derive(Debug)]
    pub struct ValidatorServer<T: Validator> {
        inner: _Inner<T>,
        accept_compression_encodings: (),
        send_compression_encodings: (),
    }
    struct _Inner<T>(Arc<T>);
    impl<T: Validator> ValidatorServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for ValidatorServer<T>
    where
        T: Validator,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/narwhal.Validator/GetCollections" => {
                    #[allow(non_camel_case_types)]
                    struct GetCollectionsSvc<T: Validator>(pub Arc<T>);
                    impl<
                        T: Validator,
                    > tonic::server::UnaryService<super::GetCollectionsRequest>
                    for GetCollectionsSvc<T> {
                        type Response = super::GetCollectionsResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::GetCollectionsRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).get_collections(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = GetCollectionsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/narwhal.Validator/RemoveCollections" => {
                    #[allow(non_camel_case_types)]
                    struct RemoveCollectionsSvc<T: Validator>(pub Arc<T>);
                    impl<
                        T: Validator,
                    > tonic::server::UnaryService<super::RemoveCollectionsRequest>
                    for RemoveCollectionsSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::RemoveCollectionsRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).remove_collections(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = RemoveCollectionsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/narwhal.Validator/ReadCausal" => {
                    #[allow(non_camel_case_types)]
                    struct ReadCausalSvc<T: Validator>(pub Arc<T>);
                    impl<
                        T: Validator,
                    > tonic::server::UnaryService<super::ReadCausalRequest>
                    for ReadCausalSvc<T> {
                        type Response = super::ReadCausalResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::ReadCausalRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).read_causal(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = ReadCausalSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: Validator> Clone for ValidatorServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: Validator> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: Validator> tonic::transport::NamedService for ValidatorServer<T> {
        const NAME: &'static str = "narwhal.Validator";
    }
}
/// Generated server implementations.
pub mod proposer_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    ///Generated trait containing gRPC methods that should be implemented for use with ProposerServer.
    #[async_trait]
    pub trait Proposer: Send + Sync + 'static {
        async fn rounds(
            &self,
            request: tonic::Request<super::RoundsRequest>,
        ) -> Result<tonic::Response<super::RoundsResponse>, tonic::Status>;
        /// Returns the read_causal obtained by starting the DAG walk at the collection
        /// proposed by the input authority (as indicated by their public key) at the input round
        async fn node_read_causal(
            &self,
            request: tonic::Request<super::NodeReadCausalRequest>,
        ) -> Result<tonic::Response<super::NodeReadCausalResponse>, tonic::Status>;
    }
    //// The API that hosts the endpoints that should be used to help
    //// proposing a block.
    #[derive(Debug)]
    pub struct ProposerServer<T: Proposer> {
        inner: _Inner<T>,
        accept_compression_encodings: (),
        send_compression_encodings: (),
    }
    struct _Inner<T>(Arc<T>);
    impl<T: Proposer> ProposerServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for ProposerServer<T>
    where
        T: Proposer,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/narwhal.Proposer/Rounds" => {
                    #[allow(non_camel_case_types)]
                    struct RoundsSvc<T: Proposer>(pub Arc<T>);
                    impl<T: Proposer> tonic::server::UnaryService<super::RoundsRequest>
                    for RoundsSvc<T> {
                        type Response = super::RoundsResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::RoundsRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).rounds(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = RoundsSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/narwhal.Proposer/NodeReadCausal" => {
                    #[allow(non_camel_case_types)]
                    struct NodeReadCausalSvc<T: Proposer>(pub Arc<T>);
                    impl<
                        T: Proposer,
                    > tonic::server::UnaryService<super::NodeReadCausalRequest>
                    for NodeReadCausalSvc<T> {
                        type Response = super::NodeReadCausalResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::NodeReadCausalRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).node_read_causal(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = NodeReadCausalSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: Proposer> Clone for ProposerServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: Proposer> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: Proposer> tonic::transport::NamedService for ProposerServer<T> {
        const NAME: &'static str = "narwhal.Proposer";
    }
}
/// Generated server implementations.
pub mod configuration_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    ///Generated trait containing gRPC methods that should be implemented for use with ConfigurationServer.
    #[async_trait]
    pub trait Configuration: Send + Sync + 'static {
        /// Signals a new epoch
        async fn new_epoch(
            &self,
            request: tonic::Request<super::NewEpochRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        /// Signals a change in networking info
        async fn new_network_info(
            &self,
            request: tonic::Request<super::NewNetworkInfoRequest>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
    }
    #[derive(Debug)]
    pub struct ConfigurationServer<T: Configuration> {
        inner: _Inner<T>,
        accept_compression_encodings: (),
        send_compression_encodings: (),
    }
    struct _Inner<T>(Arc<T>);
    impl<T: Configuration> ConfigurationServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for ConfigurationServer<T>
    where
        T: Configuration,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/narwhal.Configuration/NewEpoch" => {
                    #[allow(non_camel_case_types)]
                    struct NewEpochSvc<T: Configuration>(pub Arc<T>);
                    impl<
                        T: Configuration,
                    > tonic::server::UnaryService<super::NewEpochRequest>
                    for NewEpochSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::NewEpochRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).new_epoch(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = NewEpochSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/narwhal.Configuration/NewNetworkInfo" => {
                    #[allow(non_camel_case_types)]
                    struct NewNetworkInfoSvc<T: Configuration>(pub Arc<T>);
                    impl<
                        T: Configuration,
                    > tonic::server::UnaryService<super::NewNetworkInfoRequest>
                    for NewNetworkInfoSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::NewNetworkInfoRequest>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).new_network_info(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = NewNetworkInfoSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: Configuration> Clone for ConfigurationServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: Configuration> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: Configuration> tonic::transport::NamedService for ConfigurationServer<T> {
        const NAME: &'static str = "narwhal.Configuration";
    }
}
/// Generated server implementations.
pub mod primary_to_primary_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    ///Generated trait containing gRPC methods that should be implemented for use with PrimaryToPrimaryServer.
    #[async_trait]
    pub trait PrimaryToPrimary: Send + Sync + 'static {
        /// Sends a message
        async fn send_message(
            &self,
            request: tonic::Request<super::BincodeEncodedPayload>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
    }
    /// The primary-to-primary interface
    #[derive(Debug)]
    pub struct PrimaryToPrimaryServer<T: PrimaryToPrimary> {
        inner: _Inner<T>,
        accept_compression_encodings: (),
        send_compression_encodings: (),
    }
    struct _Inner<T>(Arc<T>);
    impl<T: PrimaryToPrimary> PrimaryToPrimaryServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for PrimaryToPrimaryServer<T>
    where
        T: PrimaryToPrimary,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/narwhal.PrimaryToPrimary/SendMessage" => {
                    #[allow(non_camel_case_types)]
                    struct SendMessageSvc<T: PrimaryToPrimary>(pub Arc<T>);
                    impl<
                        T: PrimaryToPrimary,
                    > tonic::server::UnaryService<super::BincodeEncodedPayload>
                    for SendMessageSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::BincodeEncodedPayload>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).send_message(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SendMessageSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: PrimaryToPrimary> Clone for PrimaryToPrimaryServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: PrimaryToPrimary> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: PrimaryToPrimary> tonic::transport::NamedService
    for PrimaryToPrimaryServer<T> {
        const NAME: &'static str = "narwhal.PrimaryToPrimary";
    }
}
/// Generated server implementations.
pub mod worker_to_worker_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    ///Generated trait containing gRPC methods that should be implemented for use with WorkerToWorkerServer.
    #[async_trait]
    pub trait WorkerToWorker: Send + Sync + 'static {
        /// Sends a worker message
        async fn send_message(
            &self,
            request: tonic::Request<super::BincodeEncodedPayload>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        ///Server streaming response type for the ClientBatchRequest method.
        type ClientBatchRequestStream: futures_core::Stream<
                Item = Result<super::BincodeEncodedPayload, tonic::Status>,
            >
            + Send
            + 'static;
        /// requests a number of batches that the service then streams back to the client
        async fn client_batch_request(
            &self,
            request: tonic::Request<super::BincodeEncodedPayload>,
        ) -> Result<tonic::Response<Self::ClientBatchRequestStream>, tonic::Status>;
    }
    /// The worker-to-worker interface
    #[derive(Debug)]
    pub struct WorkerToWorkerServer<T: WorkerToWorker> {
        inner: _Inner<T>,
        accept_compression_encodings: (),
        send_compression_encodings: (),
    }
    struct _Inner<T>(Arc<T>);
    impl<T: WorkerToWorker> WorkerToWorkerServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for WorkerToWorkerServer<T>
    where
        T: WorkerToWorker,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/narwhal.WorkerToWorker/SendMessage" => {
                    #[allow(non_camel_case_types)]
                    struct SendMessageSvc<T: WorkerToWorker>(pub Arc<T>);
                    impl<
                        T: WorkerToWorker,
                    > tonic::server::UnaryService<super::BincodeEncodedPayload>
                    for SendMessageSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::BincodeEncodedPayload>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).send_message(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SendMessageSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/narwhal.WorkerToWorker/ClientBatchRequest" => {
                    #[allow(non_camel_case_types)]
                    struct ClientBatchRequestSvc<T: WorkerToWorker>(pub Arc<T>);
                    impl<
                        T: WorkerToWorker,
                    > tonic::server::ServerStreamingService<super::BincodeEncodedPayload>
                    for ClientBatchRequestSvc<T> {
                        type Response = super::BincodeEncodedPayload;
                        type ResponseStream = T::ClientBatchRequestStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::BincodeEncodedPayload>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).client_batch_request(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = ClientBatchRequestSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.server_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: WorkerToWorker> Clone for WorkerToWorkerServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: WorkerToWorker> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: WorkerToWorker> tonic::transport::NamedService for WorkerToWorkerServer<T> {
        const NAME: &'static str = "narwhal.WorkerToWorker";
    }
}
/// Generated server implementations.
pub mod worker_to_primary_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    ///Generated trait containing gRPC methods that should be implemented for use with WorkerToPrimaryServer.
    #[async_trait]
    pub trait WorkerToPrimary: Send + Sync + 'static {
        /// Sends a message
        async fn send_message(
            &self,
            request: tonic::Request<super::BincodeEncodedPayload>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
    }
    /// The worker-to-primary interface
    #[derive(Debug)]
    pub struct WorkerToPrimaryServer<T: WorkerToPrimary> {
        inner: _Inner<T>,
        accept_compression_encodings: (),
        send_compression_encodings: (),
    }
    struct _Inner<T>(Arc<T>);
    impl<T: WorkerToPrimary> WorkerToPrimaryServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for WorkerToPrimaryServer<T>
    where
        T: WorkerToPrimary,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/narwhal.WorkerToPrimary/SendMessage" => {
                    #[allow(non_camel_case_types)]
                    struct SendMessageSvc<T: WorkerToPrimary>(pub Arc<T>);
                    impl<
                        T: WorkerToPrimary,
                    > tonic::server::UnaryService<super::BincodeEncodedPayload>
                    for SendMessageSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::BincodeEncodedPayload>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).send_message(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SendMessageSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: WorkerToPrimary> Clone for WorkerToPrimaryServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: WorkerToPrimary> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: WorkerToPrimary> tonic::transport::NamedService
    for WorkerToPrimaryServer<T> {
        const NAME: &'static str = "narwhal.WorkerToPrimary";
    }
}
/// Generated server implementations.
pub mod primary_to_worker_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    ///Generated trait containing gRPC methods that should be implemented for use with PrimaryToWorkerServer.
    #[async_trait]
    pub trait PrimaryToWorker: Send + Sync + 'static {
        /// Sends a message
        async fn send_message(
            &self,
            request: tonic::Request<super::BincodeEncodedPayload>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
    }
    /// The primary-to-worker interface
    #[derive(Debug)]
    pub struct PrimaryToWorkerServer<T: PrimaryToWorker> {
        inner: _Inner<T>,
        accept_compression_encodings: (),
        send_compression_encodings: (),
    }
    struct _Inner<T>(Arc<T>);
    impl<T: PrimaryToWorker> PrimaryToWorkerServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for PrimaryToWorkerServer<T>
    where
        T: PrimaryToWorker,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/narwhal.PrimaryToWorker/SendMessage" => {
                    #[allow(non_camel_case_types)]
                    struct SendMessageSvc<T: PrimaryToWorker>(pub Arc<T>);
                    impl<
                        T: PrimaryToWorker,
                    > tonic::server::UnaryService<super::BincodeEncodedPayload>
                    for SendMessageSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::BincodeEncodedPayload>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).send_message(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SendMessageSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: PrimaryToWorker> Clone for PrimaryToWorkerServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: PrimaryToWorker> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: PrimaryToWorker> tonic::transport::NamedService
    for PrimaryToWorkerServer<T> {
        const NAME: &'static str = "narwhal.PrimaryToWorker";
    }
}
/// Generated server implementations.
pub mod transactions_server {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    ///Generated trait containing gRPC methods that should be implemented for use with TransactionsServer.
    #[async_trait]
    pub trait Transactions: Send + Sync + 'static {
        /// Submit a Transactions
        async fn submit_transaction(
            &self,
            request: tonic::Request<super::Transaction>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
        /// Submit a Transactions
        async fn submit_transaction_stream(
            &self,
            request: tonic::Request<tonic::Streaming<super::Transaction>>,
        ) -> Result<tonic::Response<super::Empty>, tonic::Status>;
    }
    #[derive(Debug)]
    pub struct TransactionsServer<T: Transactions> {
        inner: _Inner<T>,
        accept_compression_encodings: (),
        send_compression_encodings: (),
    }
    struct _Inner<T>(Arc<T>);
    impl<T: Transactions> TransactionsServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            let inner = _Inner(inner);
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for TransactionsServer<T>
    where
        T: Transactions,
        B: Body + Send + 'static,
        B::Error: Into<StdError> + Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            match req.uri().path() {
                "/narwhal.Transactions/SubmitTransaction" => {
                    #[allow(non_camel_case_types)]
                    struct SubmitTransactionSvc<T: Transactions>(pub Arc<T>);
                    impl<T: Transactions> tonic::server::UnaryService<super::Transaction>
                    for SubmitTransactionSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::Transaction>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).submit_transaction(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SubmitTransactionSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/narwhal.Transactions/SubmitTransactionStream" => {
                    #[allow(non_camel_case_types)]
                    struct SubmitTransactionStreamSvc<T: Transactions>(pub Arc<T>);
                    impl<
                        T: Transactions,
                    > tonic::server::ClientStreamingService<super::Transaction>
                    for SubmitTransactionStreamSvc<T> {
                        type Response = super::Empty;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<tonic::Streaming<super::Transaction>>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).submit_transaction_stream(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = SubmitTransactionStreamSvc(inner);
                        let codec = tonic::codec::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            );
                        let res = grpc.client_streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        Ok(
                            http::Response::builder()
                                .status(200)
                                .header("grpc-status", "12")
                                .header("content-type", "application/grpc")
                                .body(empty_body())
                                .unwrap(),
                        )
                    })
                }
            }
        }
    }
    impl<T: Transactions> Clone for TransactionsServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
            }
        }
    }
    impl<T: Transactions> Clone for _Inner<T> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<T: std::fmt::Debug> std::fmt::Debug for _Inner<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{:?}", self.0)
        }
    }
    impl<T: Transactions> tonic::transport::NamedService for TransactionsServer<T> {
        const NAME: &'static str = "narwhal.Transactions";
    }
}
