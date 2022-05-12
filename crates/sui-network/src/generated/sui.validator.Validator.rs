// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
/// Generated client implementations.
pub mod validator_client {
    #![allow(unused_variables, dead_code, missing_docs, clippy::let_unit_value)]
    use tonic::codegen::*;
    ///The Validator interface
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
        pub async fn transaction(
            &mut self,
            request: impl tonic::IntoRequest<sui_types::messages::Transaction>,
        ) -> Result<
                tonic::Response<sui_types::messages::TransactionInfoResponse>,
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
            let codec = mysten_network::codec::BincodeCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/sui.validator.Validator/Transaction",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn confirmation_transaction(
            &mut self,
            request: impl tonic::IntoRequest<sui_types::messages::CertifiedTransaction>,
        ) -> Result<
                tonic::Response<sui_types::messages::TransactionInfoResponse>,
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
            let codec = mysten_network::codec::BincodeCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/sui.validator.Validator/ConfirmationTransaction",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn consensus_transaction(
            &mut self,
            request: impl tonic::IntoRequest<sui_types::messages::ConsensusTransaction>,
        ) -> Result<
                tonic::Response<sui_types::messages::TransactionInfoResponse>,
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
            let codec = mysten_network::codec::BincodeCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/sui.validator.Validator/ConsensusTransaction",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn account_info(
            &mut self,
            request: impl tonic::IntoRequest<sui_types::messages::AccountInfoRequest>,
        ) -> Result<
                tonic::Response<sui_types::messages::AccountInfoResponse>,
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
            let codec = mysten_network::codec::BincodeCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/sui.validator.Validator/AccountInfo",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn object_info(
            &mut self,
            request: impl tonic::IntoRequest<sui_types::messages::ObjectInfoRequest>,
        ) -> Result<
                tonic::Response<sui_types::messages::ObjectInfoResponse>,
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
            let codec = mysten_network::codec::BincodeCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/sui.validator.Validator/ObjectInfo",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn transaction_info(
            &mut self,
            request: impl tonic::IntoRequest<sui_types::messages::TransactionInfoRequest>,
        ) -> Result<
                tonic::Response<sui_types::messages::TransactionInfoResponse>,
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
            let codec = mysten_network::codec::BincodeCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/sui.validator.Validator/TransactionInfo",
            );
            self.inner.unary(request.into_request(), path, codec).await
        }
        pub async fn batch_info(
            &mut self,
            request: impl tonic::IntoRequest<sui_types::messages::BatchInfoRequest>,
        ) -> Result<
                tonic::Response<
                    tonic::codec::Streaming<sui_types::messages::BatchInfoResponseItem>,
                >,
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
            let codec = mysten_network::codec::BincodeCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/sui.validator.Validator/BatchInfo",
            );
            self.inner.server_streaming(request.into_request(), path, codec).await
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
        async fn transaction(
            &self,
            request: tonic::Request<sui_types::messages::Transaction>,
        ) -> Result<
                tonic::Response<sui_types::messages::TransactionInfoResponse>,
                tonic::Status,
            >;
        async fn confirmation_transaction(
            &self,
            request: tonic::Request<sui_types::messages::CertifiedTransaction>,
        ) -> Result<
                tonic::Response<sui_types::messages::TransactionInfoResponse>,
                tonic::Status,
            >;
        async fn consensus_transaction(
            &self,
            request: tonic::Request<sui_types::messages::ConsensusTransaction>,
        ) -> Result<
                tonic::Response<sui_types::messages::TransactionInfoResponse>,
                tonic::Status,
            >;
        async fn account_info(
            &self,
            request: tonic::Request<sui_types::messages::AccountInfoRequest>,
        ) -> Result<
                tonic::Response<sui_types::messages::AccountInfoResponse>,
                tonic::Status,
            >;
        async fn object_info(
            &self,
            request: tonic::Request<sui_types::messages::ObjectInfoRequest>,
        ) -> Result<
                tonic::Response<sui_types::messages::ObjectInfoResponse>,
                tonic::Status,
            >;
        async fn transaction_info(
            &self,
            request: tonic::Request<sui_types::messages::TransactionInfoRequest>,
        ) -> Result<
                tonic::Response<sui_types::messages::TransactionInfoResponse>,
                tonic::Status,
            >;
        ///Server streaming response type for the BatchInfo method.
        type BatchInfoStream: futures_core::Stream<
                Item = Result<sui_types::messages::BatchInfoResponseItem, tonic::Status>,
            >
            + Send
            + 'static;
        async fn batch_info(
            &self,
            request: tonic::Request<sui_types::messages::BatchInfoRequest>,
        ) -> Result<tonic::Response<Self::BatchInfoStream>, tonic::Status>;
    }
    ///The Validator interface
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
                "/sui.validator.Validator/Transaction" => {
                    #[allow(non_camel_case_types)]
                    struct TransactionSvc<T: Validator>(pub Arc<T>);
                    impl<
                        T: Validator,
                    > tonic::server::UnaryService<sui_types::messages::Transaction>
                    for TransactionSvc<T> {
                        type Response = sui_types::messages::TransactionInfoResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<sui_types::messages::Transaction>,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).transaction(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TransactionSvc(inner);
                        let codec = mysten_network::codec::BincodeCodec::default();
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
                "/sui.validator.Validator/ConfirmationTransaction" => {
                    #[allow(non_camel_case_types)]
                    struct ConfirmationTransactionSvc<T: Validator>(pub Arc<T>);
                    impl<
                        T: Validator,
                    > tonic::server::UnaryService<
                        sui_types::messages::CertifiedTransaction,
                    > for ConfirmationTransactionSvc<T> {
                        type Response = sui_types::messages::TransactionInfoResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                sui_types::messages::CertifiedTransaction,
                            >,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).confirmation_transaction(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = ConfirmationTransactionSvc(inner);
                        let codec = mysten_network::codec::BincodeCodec::default();
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
                "/sui.validator.Validator/ConsensusTransaction" => {
                    #[allow(non_camel_case_types)]
                    struct ConsensusTransactionSvc<T: Validator>(pub Arc<T>);
                    impl<
                        T: Validator,
                    > tonic::server::UnaryService<
                        sui_types::messages::ConsensusTransaction,
                    > for ConsensusTransactionSvc<T> {
                        type Response = sui_types::messages::TransactionInfoResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                sui_types::messages::ConsensusTransaction,
                            >,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).consensus_transaction(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = ConsensusTransactionSvc(inner);
                        let codec = mysten_network::codec::BincodeCodec::default();
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
                "/sui.validator.Validator/AccountInfo" => {
                    #[allow(non_camel_case_types)]
                    struct AccountInfoSvc<T: Validator>(pub Arc<T>);
                    impl<
                        T: Validator,
                    > tonic::server::UnaryService<
                        sui_types::messages::AccountInfoRequest,
                    > for AccountInfoSvc<T> {
                        type Response = sui_types::messages::AccountInfoResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                sui_types::messages::AccountInfoRequest,
                            >,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).account_info(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = AccountInfoSvc(inner);
                        let codec = mysten_network::codec::BincodeCodec::default();
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
                "/sui.validator.Validator/ObjectInfo" => {
                    #[allow(non_camel_case_types)]
                    struct ObjectInfoSvc<T: Validator>(pub Arc<T>);
                    impl<
                        T: Validator,
                    > tonic::server::UnaryService<sui_types::messages::ObjectInfoRequest>
                    for ObjectInfoSvc<T> {
                        type Response = sui_types::messages::ObjectInfoResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                sui_types::messages::ObjectInfoRequest,
                            >,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).object_info(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = ObjectInfoSvc(inner);
                        let codec = mysten_network::codec::BincodeCodec::default();
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
                "/sui.validator.Validator/TransactionInfo" => {
                    #[allow(non_camel_case_types)]
                    struct TransactionInfoSvc<T: Validator>(pub Arc<T>);
                    impl<
                        T: Validator,
                    > tonic::server::UnaryService<
                        sui_types::messages::TransactionInfoRequest,
                    > for TransactionInfoSvc<T> {
                        type Response = sui_types::messages::TransactionInfoResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                sui_types::messages::TransactionInfoRequest,
                            >,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move {
                                (*inner).transaction_info(request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = TransactionInfoSvc(inner);
                        let codec = mysten_network::codec::BincodeCodec::default();
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
                "/sui.validator.Validator/BatchInfo" => {
                    #[allow(non_camel_case_types)]
                    struct BatchInfoSvc<T: Validator>(pub Arc<T>);
                    impl<
                        T: Validator,
                    > tonic::server::ServerStreamingService<
                        sui_types::messages::BatchInfoRequest,
                    > for BatchInfoSvc<T> {
                        type Response = sui_types::messages::BatchInfoResponseItem;
                        type ResponseStream = T::BatchInfoStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                sui_types::messages::BatchInfoRequest,
                            >,
                        ) -> Self::Future {
                            let inner = self.0.clone();
                            let fut = async move { (*inner).batch_info(request).await };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let inner = inner.0;
                        let method = BatchInfoSvc(inner);
                        let codec = mysten_network::codec::BincodeCodec::default();
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
        const NAME: &'static str = "sui.validator.Validator";
    }
}
