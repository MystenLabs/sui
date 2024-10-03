use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_api::{DeepBookApiOpenRpc, DeepBookApiServer};
use sui_open_rpc::Module;

use crate::sui_deepbook_indexer::PgDeepbookPersistent;

pub(crate) struct DeepBookApi {
    inner: PgDeepbookPersistent,
}

impl DeepBookApi {
    pub fn new(inner: PgDeepbookPersistent) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl DeepBookApiServer for DeepBookApi {
    async fn ping(&self) -> RpcResult<String> {
        Ok("pong".to_string())
    }
}

impl SuiRpcModule for DeepBookApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        DeepBookApiOpenRpc::module_doc()
    }
}
