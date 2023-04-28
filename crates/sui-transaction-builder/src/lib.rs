// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::result::Result;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{anyhow, bail, ensure, Ok};
use async_trait::async_trait;
use futures::future::join_all;
use move_binary_format::file_format::SignatureToken;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};

use sui_json::{resolve_move_function_args, ResolvedCallArg, SuiJsonValue};
use sui_json_rpc_types::{
    RPCTransactionRequestParams, SuiData, SuiObjectDataOptions, SuiObjectResponse, SuiRawData,
    SuiTypeTag,
};
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ObjectID, ObjectInfo, ObjectRef, ObjectType, SuiAddress};
use sui_types::error::UserInputError;
use sui_types::gas_coin::GasCoin;
use sui_types::governance::{ADD_STAKE_MUL_COIN_FUN_NAME, WITHDRAW_STAKE_FUN_NAME};
use sui_types::messages::{
    Argument, CallArg, Command, InputObjectKind, ObjectArg, TransactionData, TransactionKind,
};
use sui_types::move_package::MovePackage;
use sui_types::object::{Object, Owner};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::{
    coin, fp_ensure, SUI_FRAMEWORK_OBJECT_ID, SUI_SYSTEM_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_ID,
    SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};

#[async_trait]
pub trait DataReader {
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        object_type: StructTag,
    ) -> Result<Vec<ObjectInfo>, anyhow::Error>;

    async fn get_object_with_options(
        &self,
        object_id: ObjectID,
        options: SuiObjectDataOptions,
    ) -> Result<SuiObjectResponse, anyhow::Error>;

    async fn get_reference_gas_price(&self) -> Result<u64, anyhow::Error>;
}

#[derive(Clone)]
pub struct TransactionBuilder(Arc<dyn DataReader + Sync + Send>);

impl TransactionBuilder {
    pub fn new(data_reader: Arc<dyn DataReader + Sync + Send>) -> Self {
        Self(data_reader)
    }

    async fn select_gas(
        &self,
        signer: SuiAddress,
        input_gas: Option<ObjectID>,
        budget: u64,
        input_objects: Vec<ObjectID>,
        gas_price: u64,
    ) -> Result<ObjectRef, anyhow::Error> {
        if budget < gas_price {
            bail!("Gas budget {budget} is less than the reference gas price {gas_price}. The gas budget must be at least the current reference gas price of {gas_price}.")
        }
        if let Some(gas) = input_gas {
            self.get_object_ref(gas).await
        } else {
            let gas_objs = self.0.get_owned_objects(signer, GasCoin::type_()).await?;

            for obj in gas_objs {
                let response = self
                    .0
                    .get_object_with_options(obj.object_id, SuiObjectDataOptions::new().with_bcs())
                    .await?;
                let obj = response.object()?;
                let gas: GasCoin = bcs::from_bytes(
                    &obj.bcs
                        .as_ref()
                        .ok_or_else(|| anyhow!("bcs field is unexpectedly empty"))?
                        .try_as_move()
                        .ok_or_else(|| anyhow!("Cannot parse move object to gas object"))?
                        .bcs_bytes,
                )?;
                if !input_objects.contains(&obj.object_id) && gas.value() >= budget {
                    return Ok(obj.object_ref());
                }
            }
            Err(anyhow!("Cannot find gas coin for signer address [{signer}] with amount sufficient for the required gas amount [{budget}]."))
        }
    }

    pub async fn transfer_object(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> anyhow::Result<TransactionData> {
        let mut builder = ProgrammableTransactionBuilder::new();
        self.single_transfer_object(&mut builder, object_id, recipient)
            .await?;
        let gas_price = self.0.get_reference_gas_price().await?;
        let gas = self
            .select_gas(signer, gas, gas_budget, vec![object_id], gas_price)
            .await?;

        Ok(TransactionData::new(
            TransactionKind::programmable(builder.finish()),
            signer,
            gas,
            gas_budget,
            gas_price,
        ))
    }

    async fn single_transfer_object(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        object_id: ObjectID,
        recipient: SuiAddress,
    ) -> anyhow::Result<()> {
        builder.transfer_object(recipient, self.get_object_ref(object_id).await?)?;
        Ok(())
    }

    pub async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
        amount: Option<u64>,
    ) -> anyhow::Result<TransactionData> {
        let object = self.get_object_ref(sui_object_id).await?;
        let gas_price = self.0.get_reference_gas_price().await?;
        Ok(TransactionData::new_transfer_sui(
            recipient, signer, amount, object, gas_budget, gas_price,
        ))
    }

    pub async fn pay(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        if let Some(gas) = gas {
            if input_coins.contains(&gas) {
                return Err(anyhow!("Gas coin is in input coins of Pay transaction, use PaySui transaction instead!"));
            }
        }

        let handles: Vec<_> = input_coins
            .iter()
            .map(|id| self.get_object_ref(*id))
            .collect();
        let coin_refs = join_all(handles)
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<ObjectRef>>>()?;
        let gas_price = self.0.get_reference_gas_price().await?;
        let gas = self
            .select_gas(signer, gas, gas_budget, input_coins, gas_price)
            .await?;

        TransactionData::new_pay(
            signer, coin_refs, recipients, amounts, gas, gas_budget, gas_price,
        )
    }

    pub async fn pay_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        fp_ensure!(
            !input_coins.is_empty(),
            UserInputError::EmptyInputCoins.into()
        );

        let handles: Vec<_> = input_coins
            .into_iter()
            .map(|id| self.get_object_ref(id))
            .collect();
        let mut coin_refs = join_all(handles)
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<ObjectRef>>>()?;
        // [0] is safe because input_coins is non-empty and coins are of same length as input_coins.
        let gas_object_ref = coin_refs.remove(0);
        let gas_price = self.0.get_reference_gas_price().await?;
        TransactionData::new_pay_sui(
            signer,
            coin_refs,
            recipients,
            amounts,
            gas_object_ref,
            gas_budget,
            gas_price,
        )
    }

    pub async fn pay_all_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipient: SuiAddress,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        fp_ensure!(
            !input_coins.is_empty(),
            UserInputError::EmptyInputCoins.into()
        );

        let handles: Vec<_> = input_coins
            .into_iter()
            .map(|id| self.get_object_ref(id))
            .collect();

        let mut coin_refs = join_all(handles)
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<ObjectRef>>>()?;
        // [0] is safe because input_coins is non-empty and coins are of same length as input_coins.
        let gas_object_ref = coin_refs.remove(0);
        let gas_price = self.0.get_reference_gas_price().await?;
        Ok(TransactionData::new_pay_all_sui(
            signer,
            coin_refs,
            recipient,
            gas_object_ref,
            gas_budget,
            gas_price,
        ))
    }

    pub async fn move_call(
        &self,
        signer: SuiAddress,
        package_object_id: ObjectID,
        module: &str,
        function: &str,
        type_args: Vec<SuiTypeTag>,
        call_args: Vec<SuiJsonValue>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let mut builder = ProgrammableTransactionBuilder::new();
        self.single_move_call(
            &mut builder,
            package_object_id,
            module,
            function,
            type_args,
            call_args,
        )
        .await?;
        let pt = builder.finish();
        let input_objects = pt
            .input_objects()?
            .iter()
            .flat_map(|obj| match obj {
                InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => Some(*id),
                _ => None,
            })
            .collect();
        let gas_price = self.0.get_reference_gas_price().await?;
        let gas = self
            .select_gas(signer, gas, gas_budget, input_objects, gas_price)
            .await?;

        Ok(TransactionData::new(
            TransactionKind::programmable(pt),
            signer,
            gas,
            gas_budget,
            gas_price,
        ))
    }

    pub async fn single_move_call(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        package: ObjectID,
        module: &str,
        function: &str,
        type_args: Vec<SuiTypeTag>,
        call_args: Vec<SuiJsonValue>,
    ) -> anyhow::Result<()> {
        let module = Identifier::from_str(module)?;
        let function = Identifier::from_str(function)?;

        let type_args = type_args
            .into_iter()
            .map(|ty| ty.try_into())
            .collect::<Result<Vec<_>, _>>()?;

        let call_args = self
            .resolve_and_checks_json_args(
                builder, package, &module, &function, &type_args, call_args,
            )
            .await?;

        builder.command(Command::move_call(
            package, module, function, type_args, call_args,
        ));
        Ok(())
    }

    async fn get_object_arg(
        &self,
        id: ObjectID,
        objects: &mut BTreeMap<ObjectID, Object>,
        is_mutable_ref: bool,
    ) -> Result<ObjectArg, anyhow::Error> {
        let response = self
            .0
            .get_object_with_options(id, SuiObjectDataOptions::bcs_lossless())
            .await?;

        let obj: Object = response.into_object()?.try_into()?;
        let obj_ref = obj.compute_object_reference();
        let owner = obj.owner;
        objects.insert(id, obj);
        Ok(match owner {
            Owner::Shared {
                initial_shared_version,
            } => ObjectArg::SharedObject {
                id,
                initial_shared_version,
                mutable: is_mutable_ref,
            },
            Owner::AddressOwner(_) | Owner::ObjectOwner(_) | Owner::Immutable => {
                ObjectArg::ImmOrOwnedObject(obj_ref)
            }
        })
    }

    async fn resolve_and_checks_json_args(
        &self,
        builder: &mut ProgrammableTransactionBuilder,
        package_id: ObjectID,
        module: &Identifier,
        function: &Identifier,
        type_args: &[TypeTag],
        json_args: Vec<SuiJsonValue>,
    ) -> Result<Vec<Argument>, anyhow::Error> {
        let object = self
            .0
            .get_object_with_options(package_id, SuiObjectDataOptions::bcs_lossless())
            .await?
            .into_object()?;
        let Some(SuiRawData::Package(package)) = object.bcs else {
            bail!("Bcs field in object [{}] is missing or not a package.", package_id);
        };
        let package: MovePackage = MovePackage::new(
            package.id,
            object.version,
            package.module_map,
            ProtocolConfig::get_for_min_version().max_move_package_size(),
            package.type_origin_table,
            package.linkage_table,
        )?;

        let json_args_and_tokens = resolve_move_function_args(
            &package,
            module.clone(),
            function.clone(),
            type_args,
            json_args,
        )?;

        let mut args = Vec::new();
        let mut objects = BTreeMap::new();
        for (arg, expected_type) in json_args_and_tokens {
            args.push(match arg {
                ResolvedCallArg::Pure(p) => builder.input(CallArg::Pure(p)),

                ResolvedCallArg::Object(id) => builder.input(CallArg::Object(
                    self.get_object_arg(
                        id,
                        &mut objects,
                        matches!(expected_type, SignatureToken::MutableReference(_)),
                    )
                    .await?,
                )),

                ResolvedCallArg::ObjVec(v) => {
                    let mut object_ids = vec![];
                    for id in v {
                        object_ids.push(
                            self.get_object_arg(id, &mut objects, /* is_mutable_ref */ false)
                                .await?,
                        )
                    }
                    builder.make_obj_vec(object_ids)
                }
            }?);
        }

        Ok(args)
    }

    pub async fn publish(
        &self,
        sender: SuiAddress,
        compiled_modules: Vec<Vec<u8>>,
        dep_ids: Vec<ObjectID>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let gas_price = self.0.get_reference_gas_price().await?;
        let gas = self
            .select_gas(sender, gas, gas_budget, vec![], gas_price)
            .await?;
        Ok(TransactionData::new_module(
            sender,
            gas,
            compiled_modules,
            dep_ids,
            gas_budget,
            gas_price,
        ))
    }

    pub async fn upgrade(
        &self,
        sender: SuiAddress,
        package_id: ObjectID,
        compiled_modules: Vec<Vec<u8>>,
        dep_ids: Vec<ObjectID>,
        upgrade_capability: ObjectID,
        upgrade_policy: u8,
        digest: Vec<u8>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let gas_price = self.0.get_reference_gas_price().await?;
        let gas = self
            .select_gas(sender, gas, gas_budget, vec![], gas_price)
            .await?;
        let upgrade_cap = self
            .0
            .get_object_with_options(upgrade_capability, SuiObjectDataOptions::new().with_owner())
            .await?
            .into_object()?;
        let cap_owner = upgrade_cap
            .owner
            .ok_or_else(|| anyhow!("Unable to determine ownership of upgrade capability"))?;
        TransactionData::new_upgrade(
            sender,
            gas,
            package_id,
            compiled_modules,
            dep_ids,
            (upgrade_cap.object_ref(), cap_owner),
            upgrade_policy,
            digest,
            gas_budget,
            gas_price,
        )
    }

    // TODO: consolidate this with Pay transactions
    pub async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let coin = self
            .0
            .get_object_with_options(coin_object_id, SuiObjectDataOptions::bcs_lossless())
            .await?
            .into_object()?;
        let coin_object_ref = coin.object_ref();
        let coin: Object = coin.try_into()?;
        let type_args = vec![coin.get_move_template_type()?];
        let gas_price = self.0.get_reference_gas_price().await?;
        let gas = self
            .select_gas(signer, gas, gas_budget, vec![coin_object_id], gas_price)
            .await?;

        TransactionData::new_move_call(
            signer,
            SUI_FRAMEWORK_OBJECT_ID,
            coin::PAY_MODULE_NAME.to_owned(),
            coin::PAY_SPLIT_VEC_FUNC_NAME.to_owned(),
            type_args,
            gas,
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_object_ref)),
                CallArg::Pure(bcs::to_bytes(&split_amounts)?),
            ],
            gas_budget,
            gas_price,
        )
    }

    // TODO: consolidate this with Pay transactions
    pub async fn split_coin_equal(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_count: u64,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let coin = self
            .0
            .get_object_with_options(coin_object_id, SuiObjectDataOptions::bcs_lossless())
            .await?
            .into_object()?;
        let coin_object_ref = coin.object_ref();
        let coin: Object = coin.try_into()?;
        let type_args = vec![coin.get_move_template_type()?];
        let gas_price = self.0.get_reference_gas_price().await?;
        let gas = self
            .select_gas(signer, gas, gas_budget, vec![coin_object_id], gas_price)
            .await?;

        TransactionData::new_move_call(
            signer,
            SUI_FRAMEWORK_OBJECT_ID,
            coin::PAY_MODULE_NAME.to_owned(),
            coin::PAY_SPLIT_N_FUNC_NAME.to_owned(),
            type_args,
            gas,
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_object_ref)),
                CallArg::Pure(bcs::to_bytes(&split_count)?),
            ],
            gas_budget,
            gas_price,
        )
    }

    // TODO: consolidate this with Pay transactions
    pub async fn merge_coins(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let coin = self
            .0
            .get_object_with_options(primary_coin, SuiObjectDataOptions::bcs_lossless())
            .await?
            .into_object()?;
        let primary_coin_ref = coin.object_ref();
        let coin_to_merge_ref = self.get_object_ref(coin_to_merge).await?;
        let coin: Object = coin.try_into()?;
        let type_args = vec![coin.get_move_template_type()?];
        let gas_price = self.0.get_reference_gas_price().await?;
        let gas = self
            .select_gas(
                signer,
                gas,
                gas_budget,
                vec![primary_coin, coin_to_merge],
                gas_price,
            )
            .await?;

        TransactionData::new_move_call(
            signer,
            SUI_FRAMEWORK_OBJECT_ID,
            coin::PAY_MODULE_NAME.to_owned(),
            coin::PAY_JOIN_FUNC_NAME.to_owned(),
            type_args,
            gas,
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(primary_coin_ref)),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_to_merge_ref)),
            ],
            gas_budget,
            gas_price,
        )
    }

    pub async fn batch_transaction(
        &self,
        signer: SuiAddress,
        single_transaction_params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        fp_ensure!(
            !single_transaction_params.is_empty(),
            UserInputError::InvalidBatchTransaction {
                error: "Batch Transaction cannot be empty".to_owned(),
            }
            .into()
        );
        let mut builder = ProgrammableTransactionBuilder::new();
        for param in single_transaction_params {
            match param {
                RPCTransactionRequestParams::TransferObjectRequestParams(param) => {
                    self.single_transfer_object(&mut builder, param.object_id, param.recipient)
                        .await?
                }
                RPCTransactionRequestParams::MoveCallRequestParams(param) => {
                    self.single_move_call(
                        &mut builder,
                        param.package_object_id,
                        &param.module,
                        &param.function,
                        param.type_arguments,
                        param.arguments,
                    )
                    .await?
                }
            };
        }
        let pt = builder.finish();
        let all_inputs = pt.input_objects()?;
        let inputs = all_inputs
            .iter()
            .flat_map(|obj| match obj {
                InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => Some(*id),
                _ => None,
            })
            .collect();
        let gas_price = self.0.get_reference_gas_price().await?;
        let gas = self
            .select_gas(signer, gas, gas_budget, inputs, gas_price)
            .await?;

        Ok(TransactionData::new(
            TransactionKind::programmable(pt),
            signer,
            gas,
            gas_budget,
            gas_price,
        ))
    }

    pub async fn request_add_stake(
        &self,
        signer: SuiAddress,
        mut coins: Vec<ObjectID>,
        amount: Option<u64>,
        validator: SuiAddress,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let gas_price = self.0.get_reference_gas_price().await?;
        let gas = self
            .select_gas(signer, gas, gas_budget, coins.clone(), gas_price)
            .await?;

        let mut obj_vec = vec![];
        let coin = coins
            .pop()
            .ok_or_else(|| anyhow!("Coins input should contain at lease one coin object."))?;
        let (oref, coin_type) = self.get_object_ref_and_type(coin).await?;

        let ObjectType::Struct(type_) = &coin_type else{
            return Err(anyhow!("Provided object [{coin}] is not a move object."))
        };
        ensure!(
            type_.is_coin(),
            "Expecting either Coin<T> input coin objects. Received [{type_}]"
        );

        for coin in coins {
            let (oref, type_) = self.get_object_ref_and_type(coin).await?;
            ensure!(
                type_ == coin_type,
                "All coins should be the same type, expecting {coin_type}, got {type_}."
            );
            obj_vec.push(ObjectArg::ImmOrOwnedObject(oref))
        }
        obj_vec.push(ObjectArg::ImmOrOwnedObject(oref));

        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            let arguments = vec![
                builder
                    .input(CallArg::Object(ObjectArg::SharedObject {
                        id: SUI_SYSTEM_STATE_OBJECT_ID,
                        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                        mutable: true,
                    }))
                    .unwrap(),
                builder.make_obj_vec(obj_vec)?,
                builder
                    .input(CallArg::Pure(bcs::to_bytes(&amount)?))
                    .unwrap(),
                builder
                    .input(CallArg::Pure(bcs::to_bytes(&validator)?))
                    .unwrap(),
            ];
            builder.command(Command::move_call(
                SUI_SYSTEM_OBJECT_ID,
                SUI_SYSTEM_MODULE_NAME.to_owned(),
                ADD_STAKE_MUL_COIN_FUN_NAME.to_owned(),
                vec![],
                arguments,
            ));
            builder.finish()
        };
        Ok(TransactionData::new_programmable(
            signer,
            vec![gas],
            pt,
            gas_budget,
            gas_price,
        ))
    }

    pub async fn request_withdraw_stake(
        &self,
        signer: SuiAddress,
        staked_sui: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> anyhow::Result<TransactionData> {
        let staked_sui = self.get_object_ref(staked_sui).await?;
        let gas_price = self.0.get_reference_gas_price().await?;
        let gas = self
            .select_gas(signer, gas, gas_budget, vec![], gas_price)
            .await?;
        TransactionData::new_move_call(
            signer,
            SUI_SYSTEM_OBJECT_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            WITHDRAW_STAKE_FUN_NAME.to_owned(),
            vec![],
            gas,
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_SYSTEM_STATE_OBJECT_ID,
                    initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                    mutable: true,
                }),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(staked_sui)),
            ],
            gas_budget,
            gas_price,
        )
    }

    // TODO: we should add retrial to reduce the transaction building error rate
    async fn get_object_ref(&self, object_id: ObjectID) -> anyhow::Result<ObjectRef> {
        self.get_object_ref_and_type(object_id)
            .await
            .map(|(oref, _)| oref)
    }

    async fn get_object_ref_and_type(
        &self,
        object_id: ObjectID,
    ) -> anyhow::Result<(ObjectRef, ObjectType)> {
        let object = self
            .0
            .get_object_with_options(object_id, SuiObjectDataOptions::new().with_type())
            .await?
            .into_object()?;

        Ok((object.object_ref(), object.object_type()?))
    }
}
