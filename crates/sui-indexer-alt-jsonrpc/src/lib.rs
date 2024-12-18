// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use anyhow::Context;
use jsonrpsee::server::{RpcModule, Server, ServerBuilder};
use sui_pg_db::Db;
use tokio::task::JoinHandle;
use tracing::info;

use crate::api::governance::{GovernanceImpl, GovernanceServer};
use crate::args::Args;

mod api;
pub mod args;

#[derive(clap::Args, Debug, Clone)]
pub struct RpcArgs {
    /// Address to listen to for incoming JSON-RPC connections.
    #[clap(long)]
    listen_address: SocketAddr,
}

pub struct RpcService {
    /// The JSON-RPC server.
    server: Server,

    /// All the methods added to the server so far.
    modules: RpcModule<()>,
}

impl RpcService {
    /// Create a new instance of the JSON-RPC service, configured by `rpc_args`. The service will
    /// bind to its socket upon construction (and will fail if this is not possible), but will not
    /// accept connections until [Self::run] is called.
    pub async fn new(rpc_args: RpcArgs) -> anyhow::Result<Self> {
        let RpcArgs { listen_address } = rpc_args;

        let server = ServerBuilder::new()
            .http_only()
            .build(listen_address)
            .await
            .context("Failed to bind server")?;

        Ok(Self {
            server,
            modules: RpcModule::new(()),
        })
    }

    /// Add an [RpcModule] to the service. The module's methods are combined with the existing
    /// methods registered on the service, and the operation will fail if there is any overlap.
    pub fn add_module<M>(&mut self, module: RpcModule<M>) -> anyhow::Result<()> {
        self.modules
            .merge(module.remove_context())
            .context("Failed to add module because of a name conflict")
    }

    /// Start the service (it will accept connections) and return a handle that will resolve when
    /// the service stops.
    pub async fn run(self) -> anyhow::Result<JoinHandle<()>> {
        let Self { server, modules } = self;

        let listen_address = server
            .local_addr()
            .context("Can't start RPC service without listen address")?;

        info!("Starting JSON-RPC service on {listen_address}",);

        let handle = server
            .start(modules)
            .context("Failed to start JSON-RPC service")?;

        Ok(tokio::spawn(async move {
            handle.stopped().await;
        }))
    }
}

pub async fn start_rpc(args: Args) -> anyhow::Result<()> {
    let Args { db_args, rpc_args } = args;

    let mut rpc = RpcService::new(rpc_args)
        .await
        .context("Failed to start RPC service")?;

    let db = Db::for_read(db_args)
        .await
        .context("Failed to connect to database")?;

    rpc.add_module(GovernanceImpl(db.clone()).into_rpc())?;

    let h_rpc = rpc.run().await.context("Failed to start RPC service")?;
    let _ = h_rpc.await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        net::{IpAddr, Ipv4Addr, SocketAddr},
    };

    use jsonrpsee::{core::RpcResult, proc_macros::rpc};
    use sui_pg_db::temp::get_available_port;

    use super::*;

    fn test_listen_address() -> SocketAddr {
        let port = get_available_port();
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
    }

    async fn test_service() -> RpcService {
        let listen_address = test_listen_address();
        RpcService::new(RpcArgs { listen_address })
            .await
            .expect("Failed to create test JSON-RPC service")
    }

    #[tokio::test]
    async fn test_add_module() {
        let mut rpc = test_service().await;

        #[rpc(server, namespace = "test")]
        trait Foo {
            #[method(name = "bar")]
            fn bar(&self) -> RpcResult<u64>;
        }

        struct FooImpl;

        impl FooServer for FooImpl {
            fn bar(&self) -> RpcResult<u64> {
                Ok(42)
            }
        }

        rpc.add_module(FooImpl.into_rpc()).unwrap();

        assert_eq!(
            BTreeSet::from_iter(rpc.modules.method_names()),
            BTreeSet::from_iter(["test_bar"]),
        )
    }

    #[tokio::test]
    async fn test_add_module_multiple_methods() {
        let mut rpc = test_service().await;

        #[rpc(server, namespace = "test")]
        trait Foo {
            #[method(name = "bar")]
            fn bar(&self) -> RpcResult<u64>;

            #[method(name = "baz")]
            fn baz(&self) -> RpcResult<u64>;
        }

        struct FooImpl;

        impl FooServer for FooImpl {
            fn bar(&self) -> RpcResult<u64> {
                Ok(42)
            }

            fn baz(&self) -> RpcResult<u64> {
                Ok(43)
            }
        }

        rpc.add_module(FooImpl.into_rpc()).unwrap();

        assert_eq!(
            BTreeSet::from_iter(rpc.modules.method_names()),
            BTreeSet::from_iter(["test_bar", "test_baz"]),
        )
    }

    #[tokio::test]
    async fn test_add_multiple_modules() {
        let mut rpc = test_service().await;

        #[rpc(server, namespace = "test")]
        trait Foo {
            #[method(name = "bar")]
            fn bar(&self) -> RpcResult<u64>;
        }

        #[rpc(server, namespace = "test")]
        trait Bar {
            #[method(name = "baz")]
            fn baz(&self) -> RpcResult<u64>;
        }

        struct FooImpl;
        struct BarImpl;

        impl FooServer for FooImpl {
            fn bar(&self) -> RpcResult<u64> {
                Ok(42)
            }
        }

        impl BarServer for BarImpl {
            fn baz(&self) -> RpcResult<u64> {
                Ok(43)
            }
        }

        rpc.add_module(FooImpl.into_rpc()).unwrap();
        rpc.add_module(BarImpl.into_rpc()).unwrap();

        assert_eq!(
            BTreeSet::from_iter(rpc.modules.method_names()),
            BTreeSet::from_iter(["test_bar", "test_baz"]),
        )
    }

    #[tokio::test]
    async fn test_add_module_conflict() {
        let mut rpc = test_service().await;

        #[rpc(server, namespace = "test")]
        trait Foo {
            #[method(name = "bar")]
            fn bar(&self) -> RpcResult<u64>;
        }

        #[rpc(server, namespace = "test")]
        trait Bar {
            #[method(name = "bar")]
            fn bar(&self) -> RpcResult<u64>;
        }

        struct FooImpl;
        struct BarImpl;

        impl FooServer for FooImpl {
            fn bar(&self) -> RpcResult<u64> {
                Ok(42)
            }
        }

        impl BarServer for BarImpl {
            fn bar(&self) -> RpcResult<u64> {
                Ok(43)
            }
        }

        rpc.add_module(FooImpl.into_rpc()).unwrap();
        assert!(rpc.add_module(BarImpl.into_rpc()).is_err(),)
    }
}
