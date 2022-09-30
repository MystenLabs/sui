use std::{path::Path, str::FromStr, collections::{HashSet, HashMap}};

use move_core_types::account_address::AccountAddress;
use move_package::BuildConfig;

use sui_sdk::{SuiClient, rpc_types::{SuiRawData, SuiRawMoveObject}};
use sui_types::{base_types::{ObjectID, ObjectIDParseError}, error::SuiError};

#[derive(Clone, Debug)]
pub struct VerificationResult {
    pub verified_dependencies: HashSet<Dependency>
}

#[derive(Debug)]
pub enum VerificationError {
    RpcCreationFailure(anyhow::Error),
    ResolutionGraphNotResolved(anyhow::Error),
    ObjectIdFromAddressFailure(ObjectIDParseError),
    DependencyObjectReadFailure(anyhow::Error),
    SuiObjectRefFailure(SuiError),
    ObjectFoundWhenPackageExpected(ObjectID, SuiRawMoveObject),
    ModuleCountMismatch(usize, usize),                                      // expected, found
    ModuleBytecodeMismatch(String, AccountAddress, Vec<u8>, Vec<u8>),         // name, expected, found
}

pub struct BytecodeSourceVerifier {
    rpc_client: SuiClient
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct Dependency {
    pub symbol: String,
    pub address: AccountAddress,
    pub module_bytes: Vec<Vec<u8>>
}

impl BytecodeSourceVerifier {

    pub async fn new(sui_node_url: &str) -> Result<Self, anyhow::Error> {
        let rpc_client = match SuiClient::new_rpc_client(sui_node_url, None).await {
            Ok(client) => client,
            Err(err) => { return Err(err) }
        };

        Ok(BytecodeSourceVerifier { rpc_client })
    }

    pub async fn verify_deployed_dependencies(&self, build_config: &BuildConfig, path: &Path, compiled_modules: &Vec<Vec<u8>>)
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

        let mut verified_deps: HashMap<AccountAddress, Dependency> = HashMap::new();
        for symbol_package in resolution_graph.package_table {
            println!("\nresolution graph symbol: {:#?}", symbol_package.0);

            let resolution_package = symbol_package.1;
            println!("\nresolution package: {:#?}\n", resolution_package);

            for dep in resolution_package.resolution_table {
                let symbol = dep.0;
                let addr = dep.1;
                // zero address is the package we're checking
                if addr.eq(&AccountAddress::ZERO) { continue; }
                // package addresses may show up many times, but we only need to verify them once
                if verified_deps.contains_key(&addr) { continue; }

                // Move packages are specified with an AccountAddress, but are
                // fetched from a sui network via sui_getObject, which takes an object ID
                let obj_id = match ObjectID::from_str(addr.to_string().as_str()) {
                    Ok(id) => id,
                    Err(err) => return Err(VerificationError::ObjectIdFromAddressFailure(err))
                };

                let obj_read = match self.rpc_client
                    .read_api()
                    .get_object(obj_id).await {
                    Ok(raw) => raw,
                    Err(err) => return Err(VerificationError::DependencyObjectReadFailure(err))
                };

                let obj = match obj_read.object() {
                    Ok(sui_obj) => sui_obj,
                    Err(err) => return Err(VerificationError::SuiObjectRefFailure(err))
                };

                println!("\nfetched data for Move package @ {}:\n{:#?}\n", addr, &obj.data);

                let raw_package = match &obj.data {
                    SuiRawData::Package(pkg) => pkg,
                    SuiRawData::MoveObject(move_obj) => {
                        return Err(VerificationError::ObjectFoundWhenPackageExpected(obj_id, move_obj.clone()));
                    },
                };

                // TODO - is it possible not to rely on the order here ?
                let modules: Vec<(&String, &Vec<u8>)> = raw_package.module_map
                    .iter()
                    .collect();

                if modules.len() != compiled_modules.len() {
                    return Err(VerificationError::ModuleCountMismatch(compiled_modules.len(), modules.len()))
                }

                let module_comparisons = modules.iter().zip(compiled_modules.iter());

                for pair in module_comparisons {
                    let on_chain = pair.0;
                    let mod_name = on_chain.0;
                    let on_chain_bytes = on_chain.1;
                    let local_bytes = pair.1;

                    if local_bytes != on_chain_bytes {
                        return Err(VerificationError::ModuleBytecodeMismatch
                            (mod_name.clone(), addr.clone(), local_bytes.clone(), on_chain_bytes.clone()));
                    }
                }

                let address = addr.clone();
                verified_deps.insert(address, Dependency {
                    symbol: symbol.to_string(),
                    address,
                    module_bytes: compiled_modules.clone()
                });
            }
        }

        let verified_dependencies: HashSet<Dependency> = HashSet::from_iter(verified_deps
            .iter()
            .map(|(_addr, dep)| dep.clone() ));

        Ok(VerificationResult { verified_dependencies })
    }
}
