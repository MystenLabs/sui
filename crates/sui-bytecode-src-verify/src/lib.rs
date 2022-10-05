use std::{path::Path, str::FromStr, collections::{HashSet, HashMap}};

use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;
use move_package::{BuildConfig, compilation::compiled_package::CompiledPackage};

use sui_sdk::{SuiClient, rpc_types::{SuiRawData, SuiRawMoveObject}};
use sui_types::{base_types::{ObjectID, ObjectIDParseError}, error::SuiError, sui_serde::{Hex, Encoding}};

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
    // package, module, address, expected, found
    ModuleBytecodeMismatch(String, String, AccountAddress, Vec<u8>, Vec<u8>)
}

pub struct BytecodeSourceVerifier {
    rpc_client: SuiClient
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct Dependency {
    pub symbol: String,
    pub address: AccountAddress,
    pub module_bytes: Vec<(String, Vec<u8>)>
}

impl BytecodeSourceVerifier {

    pub async fn new(sui_node_url: &str) -> Result<Self, anyhow::Error> {
        let rpc_client = match SuiClient::new_rpc_client(sui_node_url, None).await {
            Ok(client) => client,
            Err(err) => { return Err(err) }
        };

        Ok(BytecodeSourceVerifier { rpc_client })
    }

    pub async fn verify_deployed_dependencies(&self, build_config: &BuildConfig, path: &Path, compiled_package: CompiledPackage)
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

        //println!("\ncompiled package dependency bytecode:  {:#?}", compiled_package.deps_compiled_units);

        let compiled_dep_modules = compiled_package.deps_compiled_units;

        let mut compiled_dep_map: HashMap<Symbol, HashMap<Symbol, Vec<u8>>> = HashMap::new();
        compiled_dep_modules
            .iter()
            .for_each(|dep_pair| {
                let unit_src = &dep_pair.1;
                let symbol = dep_pair.0;
                let name = unit_src.unit.name();
                let bytes = unit_src.unit.serialize(None);

                match compiled_dep_map.get_mut(&symbol) {
                    Some(existing_modules) => {
                        // TODO - check for existing here ? will silently overwrite
                        existing_modules.insert(name, bytes);
                    },
                    None => {
                        let mut new_map = HashMap::new();
                        new_map.insert(name, bytes);
                        compiled_dep_map.insert(symbol, new_map);
                    },
                }
            });

        let mut on_chain_module_count = 0usize;
        let mut verified_deps: HashMap<AccountAddress, Dependency> = HashMap::new();

        for symbol_package in resolution_graph.package_table {
            let resolution_package = symbol_package.1;
            let outer_symbol = symbol_package.0;

            let local_pkg_bytes = match compiled_dep_map.get(&outer_symbol) {
                Some(bytes) => {
                    println!("\nlocal package dependency {} : {} modules\n", outer_symbol.to_string(), bytes.len());
                    bytes
                },
                None => {
                    eprintln!("no local compiled package found for {}", outer_symbol);
                    continue;
                },
            };

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

                //println!("\nfetched data for Move package @ {}:\n{:#?}\n", addr, &obj.data);

                let raw_package = match &obj.data {
                    SuiRawData::Package(pkg) => pkg,
                    SuiRawData::MoveObject(move_obj) => {
                        return Err(VerificationError::ObjectFoundWhenPackageExpected(obj_id, move_obj.clone()));
                    },
                };

                let on_chain_modules: Vec<(String, Vec<u8>)> = raw_package.module_map
                    .iter()
                    .map(|p| (p.0.to_owned(), (p.1).clone()))
                    .collect();

                on_chain_module_count += on_chain_modules.len();

                for oc_pair in &on_chain_modules {
                    let pair = oc_pair.clone();
                    let oc_name = pair.0;
                    let oc_bytes = pair.1;
                    let oc_symbol = Symbol::from(oc_name.as_str());

                    match local_pkg_bytes.get(&oc_symbol) {
                        Some(local_mod_bytes) => {
                            // compare local bytes to on-chain bytes
                            if *local_mod_bytes != *oc_bytes {
                                return Err(Self::get_mismatch_error
                                    (&outer_symbol, &oc_symbol, &addr, local_mod_bytes, &oc_bytes));
                            }

                            println!("{}::{} - {} bytes, MATCH", outer_symbol, oc_name, oc_bytes.len());
                        },
                        None => {
                            eprintln!("no local module '{}::{}' found", outer_symbol, oc_name);
                        },
                    }
                }

                let address = addr.clone();
                verified_deps.insert(address, Dependency {
                    symbol: symbol.to_string(),
                    address,
                    module_bytes: on_chain_modules
                });
            }

            println!("");
        }

        if compiled_dep_modules.len() != on_chain_module_count {
            return Err(VerificationError::ModuleCountMismatch(compiled_dep_modules.len(), on_chain_module_count))
        }

        let verified_dependencies: HashSet<Dependency> = HashSet::from_iter(verified_deps
            .iter()
            .map(|(_addr, dep)| dep.clone() ));

        Ok(VerificationResult { verified_dependencies })
    }

    fn get_mismatch_error(outer_symbol: &Symbol, module: &Symbol, addr: &AccountAddress, local_bytes: &Vec<u8>, chain_bytes: &Vec<u8>) -> VerificationError {
        let pkg = outer_symbol.to_string();
        let module = module.to_string();
        let l_bytes = local_bytes.clone();
        let c_bytes = chain_bytes.clone();
        VerificationError::ModuleBytecodeMismatch(pkg, module, addr.clone(), l_bytes, c_bytes)
    }
}
