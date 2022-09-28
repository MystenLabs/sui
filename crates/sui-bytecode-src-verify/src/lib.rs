


use std::{path::Path, str::FromStr};

use move_core_types::account_address::AccountAddress;
use move_package::BuildConfig;

use sui_sdk::SuiClient;
use sui_types::{base_types::{ObjectID, ObjectIDParseError}, error::SuiError};

pub struct VerificationResult {}

pub enum VerificationError {
    RpcCreationFailure(anyhow::Error),
    ResolutionGraphNotResolved(anyhow::Error),
    ObjectIdFromAddressFailure(ObjectIDParseError),
    DependencyObjectRead(anyhow::Error),
    SuiObjectRead(SuiError),
}

pub struct BytecodeSourceVerifier {
    rpc_client: SuiClient
}

impl BytecodeSourceVerifier {

    pub async fn new(sui_node_url: &str) -> Result<Self, anyhow::Error> {
        let rpc_client = match SuiClient::new_rpc_client(sui_node_url, None).await {
            Ok(client) => client,
            Err(err) => { return Err(err) }
        };

        Ok(BytecodeSourceVerifier { rpc_client })
    }

    pub async fn verify_deployed_dependencies(&self, build_config: &BuildConfig, path: &Path)
        -> Result<VerificationResult, VerificationError> {

        let resolution_graph = match build_config
            .clone()
            .resolution_graph_for_package(path) {
                Ok(graph) => graph,
                Err(err) => {
                    eprintln!("resolution graph error: {:#?}", err);
                    return Err(VerificationError::ResolutionGraphNotResolved(err));
                }
            };

        for symbol_package in resolution_graph.package_table {
            println!("\nresolution graph symbol: {:#?}", symbol_package.0);

            let resolution_package = symbol_package.1;
            println!("\nresolution package: {:#?}\n", resolution_package);

            for dep in resolution_package.resolution_table {
                let addr = dep.1;
                // zero address is the package we're checking
                if addr.eq(&AccountAddress::ZERO) { continue; }

                let obj_id = match ObjectID::from_str(addr.to_string().as_str()) {
                    Ok(id) => id,
                    Err(err) => return Err(VerificationError::ObjectIdFromAddressFailure(err))
                };

                let obj_read = match self.rpc_client
                    .read_api()
                    .get_object(obj_id).await {
                    Ok(raw) => raw,
                    Err(err) => return Err(VerificationError::DependencyObjectRead(err))
                };

                let obj = match obj_read.object() {
                    Ok(o) => o,
                    Err(err) => return Err(VerificationError::SuiObjectRead(err))
                };

                println!("\nfetched data for Move package @ {}:\n{:#?}\n", addr, obj.data);
            }
        }

        Ok(VerificationResult {  })
    }
}
