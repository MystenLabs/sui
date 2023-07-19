// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::result::Result;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use error::{SuiTransactionBuilderError as STBError, SuiTransactionBuilderResult};
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
use sui_types::gas_coin::GasCoin;
use sui_types::governance::{ADD_STAKE_MUL_COIN_FUN_NAME, WITHDRAW_STAKE_FUN_NAME};
use sui_types::move_package::MovePackage;
use sui_types::object::{Object, Owner};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{
    Argument, CallArg, Command, InputObjectKind, ObjectArg, TransactionData, TransactionKind,
};
use sui_types::{coin, fp_ensure, SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_PACKAGE_ID};

pub mod error;

#[async_trait]
pub trait DataReader {
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        object_type: StructTag,
    ) -> anyhow::Result<Vec<ObjectInfo>>;

    async fn get_object_with_options(
        &self,
        object_id: ObjectID,
        options: SuiObjectDataOptions,
    ) -> anyhow::Result<SuiObjectResponse>;

    async fn get_reference_gas_price(&self) -> anyhow::Result<u64>;
}

#[derive(Clone)]
pub struct TransactionBuilder(Arc<dyn DataReader + Sync + Send>);

impl TransactionBuilder {
    pub fn new(data_reader: Arc<dyn DataReader + Sync + Send>) -> Self {
        Self(data_reader)
    }

    async fn get_coin_refs(
        &self,
        input_coins: &[ObjectID],
    ) -> SuiTransactionBuilderResult<Vec<ObjectRef>> {
        let handles: Vec<_> = input_coins
            .iter()
            .map(|id| self.get_object_ref(*id))
            .collect();
        join_all(handles)
            .await
            .into_iter()
            .collect::<SuiTransactionBuilderResult<Vec<ObjectRef>>>()
    }

    async fn select_gas(
        &self,
        signer: SuiAddress,
        input_gas: Option<ObjectID>,
        budget: u64,
        input_objects: Vec<ObjectID>,
        gas_price: u64,
    ) -> SuiTransactionBuilderResult<ObjectRef> {
        if budget < gas_price {
            return Err(STBError::InsufficientGasBudget(budget, gas_price));
        }
        if let Some(gas) = input_gas {
            self.get_object_ref(gas).await
        } else {
            let gas_objs = self
                .0
                .get_owned_objects(signer, GasCoin::type_())
                .await
                .map_err(STBError::DataReaderError)?;

            for obj in gas_objs {
                let response = self
                    .0
                    .get_object_with_options(obj.object_id, SuiObjectDataOptions::new().with_bcs())
                    .await
                    .map_err(STBError::DataReaderError)?;
                let obj = response.object()?;
                let gas: GasCoin = bcs::from_bytes(
                    &obj.bcs
                        .as_ref()
                        .ok_or_else(|| STBError::BcsFieldEmpty)?
                        .try_as_move()
                        .ok_or_else(|| STBError::ParseMoveObjectError)?
                        .bcs_bytes,
                )?;
                if !input_objects.contains(&obj.object_id) && gas.value() >= budget {
                    return Ok(obj.object_ref());
                }
            }
            Err(STBError::InsufficientGasCoin(signer, budget))
        }
    }

    pub async fn transfer_object(
        &self,
        signer: SuiAddress,
        object_id: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
        recipient: SuiAddress,
    ) -> SuiTransactionBuilderResult<TransactionData> {
        let mut builder = ProgrammableTransactionBuilder::new();
        self.single_transfer_object(&mut builder, object_id, recipient)
            .await?;
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
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
    ) -> SuiTransactionBuilderResult<()> {
        builder
            .transfer_object(recipient, self.get_object_ref(object_id).await?)
            .map_err(STBError::ProgrammableTransactionBuilderError)?;
        Ok(())
    }

    pub async fn transfer_sui(
        &self,
        signer: SuiAddress,
        sui_object_id: ObjectID,
        gas_budget: u64,
        recipient: SuiAddress,
        amount: Option<u64>,
    ) -> SuiTransactionBuilderResult<TransactionData> {
        let object = self.get_object_ref(sui_object_id).await?;
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
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
    ) -> SuiTransactionBuilderResult<TransactionData> {
        if let Some(gas) = gas {
            if input_coins.contains(&gas) {
                return Err(STBError::InvalidPayTransaction);
            }
        }

        let coin_refs = self.get_coin_refs(&input_coins).await?;
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
        let gas = self
            .select_gas(signer, gas, gas_budget, input_coins, gas_price)
            .await?;

        TransactionData::new_pay(
            signer, coin_refs, recipients, amounts, gas, gas_budget, gas_price,
        )
        .map_err(STBError::ProgrammableTransactionBuilderError)
    }

    pub async fn pay_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipients: Vec<SuiAddress>,
        amounts: Vec<u64>,
        gas_budget: u64,
    ) -> SuiTransactionBuilderResult<TransactionData> {
        fp_ensure!(!input_coins.is_empty(), STBError::EmptyInputCoins);

        let mut coin_refs = self.get_coin_refs(&input_coins).await?;
        // [0] is safe because input_coins is non-empty and coins are of same length as input_coins.
        let gas_object_ref = coin_refs.remove(0);
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
        TransactionData::new_pay_sui(
            signer,
            coin_refs,
            recipients,
            amounts,
            gas_object_ref,
            gas_budget,
            gas_price,
        )
        .map_err(STBError::ProgrammableTransactionBuilderError)
    }

    pub async fn pay_all_sui(
        &self,
        signer: SuiAddress,
        input_coins: Vec<ObjectID>,
        recipient: SuiAddress,
        gas_budget: u64,
    ) -> SuiTransactionBuilderResult<TransactionData> {
        fp_ensure!(!input_coins.is_empty(), STBError::EmptyInputCoins);

        let mut coin_refs = self.get_coin_refs(&input_coins).await?;

        // [0] is safe because input_coins is non-empty and coins are of same length as input_coins.
        let gas_object_ref = coin_refs.remove(0);
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
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
    ) -> SuiTransactionBuilderResult<TransactionData> {
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
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
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
    ) -> SuiTransactionBuilderResult<()> {
        let module = Identifier::from_str(module).map_err(STBError::IdentifierError)?;
        let function = Identifier::from_str(function).map_err(STBError::IdentifierError)?;

        let type_args = type_args
            .into_iter()
            .map(|ty| ty.try_into())
            .collect::<Result<Vec<_>, _>>()
            .map_err(STBError::TypeTagError)?;

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
    ) -> SuiTransactionBuilderResult<ObjectArg> {
        let response = self
            .0
            .get_object_with_options(id, SuiObjectDataOptions::bcs_lossless())
            .await
            .map_err(STBError::DataReaderError)?;

        let obj: Object = response
            .into_object()?
            .try_into()
            .map_err(STBError::SuiObjectDataError)?;
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
    ) -> SuiTransactionBuilderResult<Vec<Argument>> {
        let object = self
            .0
            .get_object_with_options(package_id, SuiObjectDataOptions::bcs_lossless())
            .await
            .map_err(STBError::DataReaderError)?
            .into_object()?;
        let Some(SuiRawData::Package(package)) = object.bcs else {
            return Err(STBError::MissingBcsField(package_id));
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
        )
        .map_err(STBError::SuiJsonError)?;

        let mut args = Vec::new();
        let mut objects = BTreeMap::new();
        for (arg, expected_type) in json_args_and_tokens {
            args.push(
                match arg {
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
                                self.get_object_arg(
                                    id,
                                    &mut objects,
                                    /* is_mutable_ref */ false,
                                )
                                .await?,
                            )
                        }
                        builder.make_obj_vec(object_ids)
                    }
                }
                .map_err(STBError::ProgrammableTransactionBuilderError)?,
            );
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
    ) -> SuiTransactionBuilderResult<TransactionData> {
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
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
    ) -> SuiTransactionBuilderResult<TransactionData> {
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
        let gas = self
            .select_gas(sender, gas, gas_budget, vec![], gas_price)
            .await?;
        let upgrade_cap = self
            .0
            .get_object_with_options(upgrade_capability, SuiObjectDataOptions::new().with_owner())
            .await
            .map_err(STBError::DataReaderError)?
            .into_object()?;
        let cap_owner = upgrade_cap
            .owner
            .ok_or_else(|| STBError::UnknownUpgradeCapability)?;
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
        .map_err(STBError::TransactionDataError)
    }

    // TODO: consolidate this with Pay transactions
    pub async fn split_coin(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_amounts: Vec<u64>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> SuiTransactionBuilderResult<TransactionData> {
        let coin = self
            .0
            .get_object_with_options(coin_object_id, SuiObjectDataOptions::bcs_lossless())
            .await
            .map_err(STBError::DataReaderError)?
            .into_object()?;
        let coin_object_ref = coin.object_ref();
        let coin: Object = coin.try_into().map_err(STBError::SuiObjectDataError)?;
        let type_args = vec![coin.get_move_template_type()?];
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
        let gas = self
            .select_gas(signer, gas, gas_budget, vec![coin_object_id], gas_price)
            .await?;

        TransactionData::new_move_call(
            signer,
            SUI_FRAMEWORK_PACKAGE_ID,
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
        .map_err(STBError::TransactionDataError)
    }

    // TODO: consolidate this with Pay transactions
    pub async fn split_coin_equal(
        &self,
        signer: SuiAddress,
        coin_object_id: ObjectID,
        split_count: u64,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> SuiTransactionBuilderResult<TransactionData> {
        let coin = self
            .0
            .get_object_with_options(coin_object_id, SuiObjectDataOptions::bcs_lossless())
            .await
            .map_err(STBError::DataReaderError)?
            .into_object()?;
        let coin_object_ref = coin.object_ref();
        let coin: Object = coin.try_into().map_err(STBError::SuiObjectDataError)?;
        let type_args = vec![coin.get_move_template_type()?];
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
        let gas = self
            .select_gas(signer, gas, gas_budget, vec![coin_object_id], gas_price)
            .await?;

        TransactionData::new_move_call(
            signer,
            SUI_FRAMEWORK_PACKAGE_ID,
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
        .map_err(STBError::TransactionDataError)
    }

    // TODO: consolidate this with Pay transactions
    pub async fn merge_coins(
        &self,
        signer: SuiAddress,
        primary_coin: ObjectID,
        coin_to_merge: ObjectID,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> SuiTransactionBuilderResult<TransactionData> {
        let coin = self
            .0
            .get_object_with_options(primary_coin, SuiObjectDataOptions::bcs_lossless())
            .await
            .map_err(STBError::DataReaderError)?
            .into_object()?;
        let primary_coin_ref = coin.object_ref();
        let coin_to_merge_ref = self.get_object_ref(coin_to_merge).await?;
        let coin: Object = coin.try_into().map_err(STBError::SuiObjectDataError)?;
        let type_args = vec![coin.get_move_template_type()?];
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
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
            SUI_FRAMEWORK_PACKAGE_ID,
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
        .map_err(STBError::TransactionDataError)
    }

    pub async fn batch_transaction(
        &self,
        signer: SuiAddress,
        single_transaction_params: Vec<RPCTransactionRequestParams>,
        gas: Option<ObjectID>,
        gas_budget: u64,
    ) -> SuiTransactionBuilderResult<TransactionData> {
        fp_ensure!(
            !single_transaction_params.is_empty(),
            STBError::InvalidBatchTransaction
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
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
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
    ) -> SuiTransactionBuilderResult<TransactionData> {
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
        let gas = self
            .select_gas(signer, gas, gas_budget, coins.clone(), gas_price)
            .await?;

        let mut obj_vec = vec![];
        let coin = coins.pop().ok_or_else(|| STBError::EmptyInputCoins)?;
        let (oref, coin_type) = self.get_object_ref_and_type(coin).await?;

        let ObjectType::Struct(type_) = &coin_type else{
            return Err(STBError::NotAMoveObject(coin))
        };
        fp_ensure!(
            type_.is_coin(),
            STBError::InvalidCoinObjectType(type_.to_string())
        );

        for coin in coins {
            let (oref, type_) = self.get_object_ref_and_type(coin).await?;
            fp_ensure!(
                type_ == coin_type,
                STBError::CoinTypeMismatch(coin_type, type_)
            );
            obj_vec.push(ObjectArg::ImmOrOwnedObject(oref))
        }
        obj_vec.push(ObjectArg::ImmOrOwnedObject(oref));

        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            let arguments = vec![
                builder.input(CallArg::SUI_SYSTEM_MUT).unwrap(),
                builder
                    .make_obj_vec(obj_vec)
                    .map_err(STBError::ProgrammableTransactionBuilderError)?,
                builder
                    .input(CallArg::Pure(bcs::to_bytes(&amount)?))
                    .unwrap(),
                builder
                    .input(CallArg::Pure(bcs::to_bytes(&validator)?))
                    .unwrap(),
            ];
            builder.command(Command::move_call(
                SUI_SYSTEM_PACKAGE_ID,
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
    ) -> SuiTransactionBuilderResult<TransactionData> {
        let staked_sui = self.get_object_ref(staked_sui).await?;
        let gas_price = self
            .0
            .get_reference_gas_price()
            .await
            .map_err(STBError::DataReaderError)?;
        let gas = self
            .select_gas(signer, gas, gas_budget, vec![], gas_price)
            .await?;
        TransactionData::new_move_call(
            signer,
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            WITHDRAW_STAKE_FUN_NAME.to_owned(),
            vec![],
            gas,
            vec![
                CallArg::SUI_SYSTEM_MUT,
                CallArg::Object(ObjectArg::ImmOrOwnedObject(staked_sui)),
            ],
            gas_budget,
            gas_price,
        )
        .map_err(STBError::ProgrammableTransactionBuilderError)
    }

    // TODO: we should add retrial to reduce the transaction building error rate
    async fn get_object_ref(&self, object_id: ObjectID) -> SuiTransactionBuilderResult<ObjectRef> {
        self.get_object_ref_and_type(object_id)
            .await
            .map(|(oref, _)| oref)
    }

    async fn get_object_ref_and_type(
        &self,
        object_id: ObjectID,
    ) -> SuiTransactionBuilderResult<(ObjectRef, ObjectType)> {
        let object = self
            .0
            .get_object_with_options(object_id, SuiObjectDataOptions::new().with_type())
            .await
            .map_err(STBError::DataReaderError)?
            .into_object()?;

        Ok((
            object.object_ref(),
            object.object_type().map_err(STBError::ObjectTypeError)?,
        ))
    }
}
