// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashSet};

use move_binary_format::{
    access::ModuleAccess, file_format::Visibility, normalized, CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag},
    value::MoveValue,
};
use rand::{distributions::Alphanumeric, prelude::StdRng, Rng, SeedableRng};
use sui::wallet_commands::{WalletCommandResult, WalletCommands, WalletContext};
use sui_core::{
    gateway_state::{GatewayAPI, GatewayClient},
    gateway_types::{GetObjectDataResponse, SuiExecutionStatus},
};
use sui_json::SuiJsonValue;
use sui_types::{
    base_types::{ObjectID, SUI_ADDRESS_LENGTH, TX_CONTEXT_MODULE_NAME},
    move_package::MovePackage,
    SUI_FRAMEWORK_ADDRESS,
};

// TODO: set to whatever the system max is
/// A large enough gas budget to call any function
const MAX_GAS: u64 = 5000;

/// Largest random vector we will generate
const MAX_VECTOR_LENGTH: u64 = 256;

#[derive(Eq, PartialEq, Debug, Clone)]
struct Function {
    module: ModuleId,
    name: Identifier,
}

#[derive(Debug, Clone)]
pub struct SurferState {
    // All of the modules we have seen during our exploration
    modules: BTreeMap<Identifier, CompiledModule>,
    /// All of the objects wallet we are currently using owns, organized by type
    // TODO: use `StructTag` for the type. Currently, the gateway API's will only give us a string
    inventory: BTreeMap<String, Vec<ObjectID>>,
    /// All of the shared objects we know about, also organized by type
    // TODO: use `StructTag` for the type. Currently, the gateway API's will only give us a string
    shared_objects: BTreeMap<String, Vec<ObjectID>>,
    /// Source of randomness used when inhabiting types (and possibly in other areas)
    rng: StdRng,
    /// Keeps track of what we have explored  
    stats: SurferStats,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct SurferStats {
    /// Number of packages the surfer has attempted to explore
    pub explored_packages: u64,
    /// Number of modules the surfer has attempted to explore
    pub explored_modules: u64,
    /// Number of functions the surfer has attempted to call
    pub explored_functions: u64,
    /// Number of functions the surfer tried to call, but failed
    pub failed_functions: u64,
}

impl SurferStats {
    pub fn new() -> Self {
        SurferStats {
            explored_packages: 0,
            explored_modules: 0,
            explored_functions: 0,
            failed_functions: 0,
        }
    }
}

impl SurferState {
    pub fn new(seed: [u8; 32]) -> Self {
        // TODO: populate with Sui genesis state?
        SurferState {
            modules: BTreeMap::new(),
            inventory: BTreeMap::new(),
            shared_objects: BTreeMap::new(),
            rng: StdRng::from_seed(seed),
            stats: SurferStats::new(),
        }
    }

    pub async fn add_owned_object(
        &mut self,
        obj_id: ObjectID,
        wallet: &mut WalletContext,
    ) -> Result<(), anyhow::Error> {
        let obj = wallet.gateway.get_object(obj_id).await?.into_object()?;
        let move_obj = obj
            .data
            .into_move_object()
            .expect("An address can only own Move objects");
        self.inventory
            .entry(move_obj.type_)
            .or_insert_with(Vec::new)
            .push(obj_id);
        Ok(())
    }

    async fn add_shared_object(
        &mut self,
        obj_id: ObjectID,
        wallet: &mut WalletContext,
    ) -> Result<(), anyhow::Error> {
        let obj = wallet.gateway.get_object(obj_id).await?.into_object()?;
        let move_obj = obj
            .data
            .into_move_object()
            .expect("An shared object must be a Move object");
        self.shared_objects
            .entry(move_obj.type_)
            .or_insert_with(Vec::new)
            .push(obj_id);
        Ok(())
    }

    /// Clear any previous inventory and populate the surfer's `inventory` with all the object `wallet` can sign for.
    async fn populate_inventory(
        &mut self,
        wallet: &mut WalletContext,
    ) -> Result<(), anyhow::Error> {
        println!("populating inventory");
        self.inventory.clear();
        // populate the inventory with `wallet`'s state
        // TODO: support multiple active addresses? the trickiness is that we need to decide which one to send each tx from
        let active_address = wallet.active_address()?;
        let objects = wallet
            .gateway
            .get_objects_owned_by_address(active_address)
            .await?;
        for object_info in objects {
            self.add_owned_object(object_info.object_id, wallet).await?;
        }
        Ok(())
    }

    /// Resolve `package_id` to a package object and explore each module inside the package
    pub async fn surf_package(
        &mut self,
        package_id: ObjectID,
        wallet: &mut WalletContext,
    ) -> Result<(), anyhow::Error> {
        self.populate_inventory(wallet).await?;

        if let GetObjectDataResponse::Exists(o) = wallet.gateway.get_object(package_id).await? {
            // TODO: get MovePackage directly and skip conversion to/from JSON
            let sui_package = o.data.try_as_package().expect(&format!(
                "Expected {} to be package, but found object",
                package_id
            ));
            let package = MovePackage::new(package_id, sui_package.modules());
            // TODO: make sure this is exploring the leaves of the dep graph first
            for (_name, module) in package.modules() {
                self.surf_module(package_id, module, wallet).await?;
            }
        } else {
            panic!("Invalid package ID {}", package_id)
        }
        self.stats.explored_packages += 1;

        Ok(())
    }

    async fn surf_module(
        &mut self,
        package: ObjectID,
        m: CompiledModule,
        wallet: &mut WalletContext,
    ) -> Result<(), anyhow::Error> {
        let module_name = m.name().to_owned();
        println!("Surfing module {}::{}", package, module_name);
        let module = normalized::Module::new(&m);
        self.modules.insert(module_name.to_owned(), m);

        // for convenience, get a normalized module. could also work directly over the bytecode
        // if we cared more about efficiency or wanted to do fancier analysis

        // TODO: randomize exploration order?

        let mut succeeded = HashSet::new();
        loop {
            let success_count = succeeded.len();
            for (function_name, function) in &module.exposed_functions {
                if succeeded.contains(function_name) {
                    continue;
                }

                // only explore `script` functions, since it is easy for us to call them from the outside world
                // TODO: a fancier surfer could attempt to publish new packages that call `public` functions
                match function.visibility {
                    Visibility::Script => (),
                    Visibility::Private | Visibility::Public | Visibility::Friend => continue,
                }
                if self
                    .surf_function(package, &module_name, &function_name, function, wallet)
                    .await?
                {
                    succeeded.insert(function_name);
                }
            }
            if success_count == succeeded.len() {
                // no longer making progress.
                break;
            }
        }
        self.stats.explored_functions += succeeded.len() as u64;
        self.stats.failed_functions += (module.exposed_functions.len() - succeeded.len()) as u64;

        self.stats.explored_modules += 1;

        Ok(())
    }

    /// Return `true` if we succeeded in calling the function
    pub async fn surf_function(
        &mut self,
        package_id: ObjectID,
        module_name: &IdentStr,
        function_name: &IdentStr,
        function: &normalized::Function,
        wallet: &mut WalletContext,
    ) -> Result<bool, anyhow::Error> {
        println!("  Surfing function {}", function_name);
        let type_args = Vec::new();
        // TODO: a fancier surfer should explore functions with type parameters
        if !function.type_parameters.is_empty() {
            return Ok(false);
        }
        let mut args = Vec::new();
        for typ in &function.parameters {
            // special case for `&mut TxContext`, which we do not need to inhabit
            if Self::is_tx_context_arg(typ) {
                continue;
            }

            if let Some(v) = self.inhabit_type(typ)? {
                // inhabited successfully. add to function args
                args.push(SuiJsonValue::from_move_value(&v)?)
            } else {
                println!("  Failed to inhabit function {} because we couldn't create a parameter of type {}", function_name, typ);
                // can't inhabit this type, so we can't call this function
                self.stats.explored_functions += 1;
                self.stats.failed_functions += 1;
                return Ok(false);
            }
        }
        // we inhabited all the args! now try to call the function
        if let WalletCommandResult::Call(_cert, effects) = WalletCommands::execute(
            WalletCommands::Call {
                package: package_id,
                module: module_name.to_string(),
                function: function_name.to_string(),
                type_args,
                args,
                // let wallet pick the gas object.
                // TODO: make sure it doesn't pick a Coin<SUI> object that we are using in args
                gas: None,
                // pick the largest gas budget allowed to maximize our chances of success
                gas_budget: MAX_GAS,
            },
            wallet,
        )
        .await?
        {
            // TODO: optionally persist _cert if we would like to use the surfer to generate transaction workloads
            if let SuiExecutionStatus::Failure { error, .. } = effects.status {
                println!("  Call failed: {}", error);
                Ok(false)
            } else {
                // 1. remove deleted objects from the inventory, populate inventory with newly created objects
                // TODO: we could do this in a much more efficient way, but for now just call populate_inventory()
                self.populate_inventory(wallet).await?;
                // 2. record shared objects created by this tx
                for obj in effects.created {
                    if obj.owner.is_shared() {
                        self.add_shared_object(obj.reference.object_id, wallet)
                            .await?;
                    }
                }
                // 3. TODO: record types of objects created by this function, so we know to call it again if we need
                // objects of those types in the future
                // TODO: record shared objects deleted by this tx
                Ok(true)
            }
        } else {
            unreachable!("Wallet call command should always produce call result")
        }
    }

    /// Choose a Move value that inhabits the type `t`
    fn inhabit_type(&mut self, t: &normalized::Type) -> Result<Option<MoveValue>, anyhow::Error> {
        use normalized::Type::*;

        // TODO: put random selection of non-struct Move values into the move/ repo
        return Ok(match t {
            Bool => Some(MoveValue::Bool(self.rng.gen())),
            U8 => Some(MoveValue::U8(self.rng.gen())),
            U64 => Some(MoveValue::U64(self.rng.gen())),
            U128 => Some(MoveValue::U128(self.rng.gen())),
            Address => Some(MoveValue::Address(
                AccountAddress::from(self.rng.gen::<[u8; SUI_ADDRESS_LENGTH]>()).into(),
            )),
            Struct { .. } => self
                .inhabit_struct_type(t.clone().into_struct_tag().unwrap())?
                .map(|id| MoveValue::Address(id.into())),
            Vector(inner_type) => {
                let length = self.rng.gen_range(0, MAX_VECTOR_LENGTH) as usize;
                if **inner_type == normalized::Type::U8 {
                    // vector<u8>. TODO: generate random string
                    return Ok(Some(MoveValue::vector_u8("hello".as_bytes().to_vec())));
                }

                let mut values = Vec::new();
                // TODO: might want to pick a smaller max length for vecs of ObjectID's, addresses, etc
                // TODO: might want a fast path for vectors of primitive types
                for _ in 0..length {
                    if let Some(value) = self.inhabit_type(inner_type)? {
                        values.push(value)
                    } else {
                        return Ok(None);
                    }
                }
                Some(MoveValue::Vector(values))
            }
            Reference(inner_type) | MutableReference(inner_type) => {
                // unwrap safe because the Sui entrypoint rules only allow references to objects or to TxContext, which
                // can both be converted to StructTag's
                let s = (*inner_type.clone()).into_struct_tag().unwrap();
                self.inhabit_struct_type(s)?
                    .map(|id| MoveValue::Address(id.into()))
            }
            TypeParameter(_) => unimplemented!("Inhabiting type param"),
            Signer => unreachable!("Signer is not supported in Sui"),
        });
    }

    fn inhabit_struct_type(&mut self, t: StructTag) -> Result<Option<ObjectID>, anyhow::Error> {
        // TODO: special case for ObjectID + others?

        // look for the type in our inventory and in shared objects, choose a value at random if we have one
        let type_str = t.to_string();
        if let Some(choices) = self.inventory.get_mut(&type_str) {
            let idx = self.rng.gen_range(0, choices.len());
            // TODO: if we're trying to inhabit an immutable reference, we could choose not to remove here
            // but then we need to be careful that we don't later use the ID in an immutable reference. stick
            // with the simple thing for now
            let choice = choices.remove(idx);
            return Ok(Some(choice));
        }
        // same thing, but with shared objects
        if let Some(choices) = self.shared_objects.get_mut(&type_str) {
            let idx = self.rng.gen_range(0, choices.len());
            return Ok(Some(choices[idx]));
        }

        println!(
            "We don't have a value of type {} in the inventory--giving up",
            t
        );
        // TODO: Iterate through `modules` in an attempt to determine how to get a `t`
        // This is hard because a function signature doesn't tell you whether it will produce a `t`
        // or not--we have to do a bit of static analysis to find out, or just explore randomly and
        // hope that we discover a function that gave us a `T` when we called it before
        Ok(None)
    }

    /// Return `true` if `t` is the special `TxContext` type
    fn is_tx_context(t: &normalized::Type) -> bool {
        if let Some(s) = t.clone().into_struct_tag() {
            s.address == SUI_FRAMEWORK_ADDRESS && s.name.as_ident_str() == TX_CONTEXT_MODULE_NAME
        } else {
            false
        }
    }

    /// Return `true` if `t` is a special `&mut TxContext` arg
    // `&TxContext` shouldn't be possible, but also handling it in case it's supported in the future
    fn is_tx_context_arg(t: &normalized::Type) -> bool {
        use normalized::Type::*;
        match t {
            MutableReference(inner) | Reference(inner) => Self::is_tx_context(inner),
            _ => false,
        }
    }

    pub fn stats(&self) -> &SurferStats {
        &self.stats
    }
}
