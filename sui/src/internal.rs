// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;

pub fn sync(
    state: &mut ServerState,
    sync_params: SyncRequest,
) -> AsyncResult<HttpResponseUpdatedNoContent, HttpError> {
    Box::pin(async move {
        let address = decode_bytes_hex(sync_params.address.as_str()).map_err(|error| {
            custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Could not decode to address from hex {error}"),
            )
        })?;

        state
            .gateway
            .sync_account_state(address)
            .await
            .map_err(|err| {
                custom_http_error(
                    StatusCode::FAILED_DEPENDENCY,
                    format!("Can't create client state: {err}"),
                )
            })?;
        Ok(HttpResponseUpdatedNoContent())
    })
}

pub fn get_addresses(
    state: &mut ServerState,
) -> AsyncResult<HttpResponseOk<GetAddressResponse>, HttpError> {
    Box::pin(async {
        // TODO: Speed up sync operations by kicking them off concurrently.
        // Also need to investigate if this should be an automatic sync or manually triggered.
        let addresses = state.addresses.clone();
        for address in &addresses {
            if let Err(err) = state.gateway.sync_account_state(*address).await {
                return Err(custom_http_error(
                    StatusCode::FAILED_DEPENDENCY,
                    format!("Can't create client state: {err}"),
                ));
            }
        }
        Ok(HttpResponseOk(GetAddressResponse {
            addresses: addresses
                .into_iter()
                .map(|address| format!("{}", address))
                .collect(),
        }))
    })
}

pub fn get_objects(
    state: &mut ServerState,
    get_objects_params: GetObjectsRequest,
) -> AsyncResult<HttpResponseOk<GetObjectsResponse>, HttpError> {
    Box::pin(async {
        let address = get_objects_params.address;
        let address = &decode_bytes_hex(address.as_str()).map_err(|error| {
            custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Could not decode address from hex {error}"),
            )
        })?;

        let object_refs = state.gateway.get_owned_objects(*address);
        Ok(HttpResponseOk(GetObjectsResponse {
            objects: object_refs
                .iter()
                .map(|(object_id, sequence_number, object_digest)| Object {
                    object_id: object_id.to_string(),
                    version: format!("{:?}", sequence_number),
                    object_digest: format!("{:?}", object_digest),
                })
                .collect::<Vec<Object>>(),
        }))
    })
}

pub fn object_schema(
    state: &mut ServerState,
    object_info_params: GetObjectSchemaRequest,
) -> AsyncResult<HttpResponseOk<ObjectSchemaResponse>, HttpError> {
    Box::pin(async {
        let object_id = match ObjectID::try_from(object_info_params.object_id) {
            Ok(object_id) => object_id,
            Err(error) => {
                return Err(custom_http_error(
                    StatusCode::FAILED_DEPENDENCY,
                    format!("{error}"),
                ));
            }
        };

        let layout = match state.gateway.get_object_info(object_id).await {
            Ok(ObjectRead::Exists(_, _, layout)) => layout,
            Ok(ObjectRead::Deleted(_)) => {
                return Err(custom_http_error(
                    StatusCode::FAILED_DEPENDENCY,
                    format!("Object ({object_id}) was deleted."),
                ));
            }
            Ok(ObjectRead::NotExists(_)) => {
                return Err(custom_http_error(
                    StatusCode::FAILED_DEPENDENCY,
                    format!("Object ({object_id}) does not exist."),
                ));
            }
            Err(error) => {
                return Err(custom_http_error(
                    StatusCode::FAILED_DEPENDENCY,
                    format!("Error while getting object info: {:?}", error),
                ));
            }
        };

        match serde_json::to_value(layout) {
            Ok(schema) => Ok(HttpResponseOk(ObjectSchemaResponse { schema })),
            Err(e) => Err(custom_http_error(
                StatusCode::FAILED_DEPENDENCY,
                format!("Error while getting object info: {:?}", e),
            )),
        }
    })
}

pub fn object_info(
    state: &mut ServerState,
    object_info_params: GetObjectInfoRequest,
) -> AsyncResult<HttpResponseOk<ObjectInfoResponse>, HttpError> {
    Box::pin(async move {
        let object_id = match ObjectID::try_from(object_info_params.object_id) {
            Ok(object_id) => object_id,
            Err(error) => {
                return Err(custom_http_error(
                    StatusCode::FAILED_DEPENDENCY,
                    format!("{error}"),
                ));
            }
        };

        let (_, object, layout) = get_object_info(state, object_id).await?;
        let object_data = object.to_json(&layout).unwrap_or_else(|_| json!(""));
        Ok(HttpResponseOk(ObjectInfoResponse {
            owner: format!("{:?}", object.owner),
            version: format!("{:?}", object.version().value()),
            id: format!("{:?}", object.id()),
            readonly: format!("{:?}", object.is_read_only()),
            obj_type: object
                .data
                .type_()
                .map_or("Unknown Type".to_owned(), |type_| format!("{}", type_)),
            data: object_data,
        }))
    })
}

pub fn transfer_object(
    state: &mut ServerState,
    transfer_order_params: TransferTransactionRequest,
) -> AsyncResult<HttpResponseOk<TransactionResponse>, HttpError> {
    Box::pin(async move {
        let to_address =
            decode_bytes_hex(transfer_order_params.to_address.as_str()).map_err(|error| {
                custom_http_error(
                    StatusCode::FAILED_DEPENDENCY,
                    format!("Could not decode to address from hex {error}"),
                )
            })?;
        let object_id = ObjectID::try_from(transfer_order_params.object_id).map_err(|error| {
            custom_http_error(StatusCode::FAILED_DEPENDENCY, format!("{error}"))
        })?;
        let gas_object_id =
            ObjectID::try_from(transfer_order_params.gas_object_id).map_err(|error| {
                custom_http_error(StatusCode::FAILED_DEPENDENCY, format!("{error}"))
            })?;
        let owner =
            decode_bytes_hex(transfer_order_params.from_address.as_str()).map_err(|error| {
                custom_http_error(
                    StatusCode::FAILED_DEPENDENCY,
                    format!("Could not decode address from hex {error}"),
                )
            })?;

        let tx_signer = Box::pin(SimpleTransactionSigner {
            keystore: state.keystore.clone(),
        });

        let (cert, effects, gas_used) = match state
            .gateway
            .transfer_coin(owner, object_id, gas_object_id, to_address, tx_signer)
            .await
        {
            Ok((cert, effects)) => {
                let gas_used = match effects.status {
                    ExecutionStatus::Success { gas_used } => gas_used,
                    ExecutionStatus::Failure { gas_used, error } => {
                        return Err(custom_http_error(
                            StatusCode::FAILED_DEPENDENCY,
                            format!(
                                "Error transferring object: {:#?}, gas used {}",
                                error, gas_used
                            ),
                        ));
                    }
                };
                (cert, effects, gas_used)
            }
            Err(err) => {
                return Err(custom_http_error(
                    StatusCode::FAILED_DEPENDENCY,
                    format!("Transfer error: {err}"),
                ));
            }
        };

        let object_effects_summary = get_object_effects(state, effects).await?;

        Ok(HttpResponseOk(TransactionResponse {
            gas_used,
            object_effects_summary: json!(object_effects_summary),
            certificate: json!(cert),
        }))
    })
}

pub fn call(
    state: &mut ServerState,
    call_params: CallRequest,
) -> AsyncResult<HttpResponseOk<TransactionResponse>, HttpError> {
    Box::pin(async {
        let transaction_response = handle_move_call(call_params, state)
            .await
            .map_err(|err| custom_http_error(StatusCode::BAD_REQUEST, format!("{err}")))?;
        Ok(HttpResponseOk(transaction_response))
    })
}

async fn handle_move_call(
    call_params: CallRequest,
    state: &mut ServerState,
) -> Result<TransactionResponse, anyhow::Error> {
    let module = Identifier::from_str(&call_params.module.to_owned())?;
    let function = Identifier::from_str(&call_params.function.to_owned())?;
    let args = call_params.args;
    let type_args = call_params
        .type_args
        .unwrap_or_default()
        .iter()
        .map(|type_arg| parse_type_tag(type_arg))
        .collect::<Result<Vec<_>, _>>()?;
    let gas_budget = call_params.gas_budget;
    let gas_object_id = ObjectID::try_from(call_params.gas_object_id)?;
    let package_object_id = ObjectID::from_hex_literal(&call_params.package_object_id)?;

    let sender: SuiAddress = decode_bytes_hex(call_params.sender.as_str())?;

    let (package_object_ref, package_object, _) = get_object_info(state, package_object_id).await?;

    // Extract the input args
    let (object_ids, pure_args) =
        resolve_move_function_args(&package_object, module.clone(), function.clone(), args)?;

    info!("Resolved fn to: \n {:?} & {:?}", object_ids, pure_args);

    // Fetch all the objects needed for this call
    let mut input_objs = vec![];
    for obj_id in object_ids.clone() {
        let (_, object, _) = get_object_info(state, obj_id).await?;
        input_objs.push(object);
    }

    // Pass in the objects for a deeper check
    resolve_and_type_check(
        package_object.clone(),
        &module,
        &function,
        &type_args,
        input_objs,
        pure_args.clone(),
    )?;

    // Fetch the object info for the gas obj
    let (gas_obj_ref, _, _) = get_object_info(state, gas_object_id).await?;

    // Fetch the objects for the object args
    let mut object_args_refs = Vec::new();
    for obj_id in object_ids {
        let (object_ref, _, _) = get_object_info(state, obj_id).await?;
        object_args_refs.push(object_ref);
    }

    let tx_signer = Box::pin(SimpleTransactionSigner {
        keystore: state.keystore.clone(),
    });

    let (cert, effects, gas_used) = match state
        .gateway
        .move_call(
            sender,
            package_object_ref,
            module.to_owned(),
            function.to_owned(),
            type_args.clone(),
            gas_obj_ref,
            object_args_refs,
            // TODO: Populate shared object args. sui/issue#719
            vec![],
            pure_args,
            gas_budget,
            tx_signer,
        )
        .await
    {
        Ok((cert, effects)) => {
            let gas_used = match effects.status {
                ExecutionStatus::Success { gas_used } => gas_used,
                ExecutionStatus::Failure { gas_used, error } => {
                    let context = format!("Error calling move function, gas used {gas_used}");
                    return Err(anyhow::Error::new(error).context(context));
                }
            };
            (cert, effects, gas_used)
        }
        Err(err) => {
            return Err(err);
        }
    };

    let object_effects_summary = get_object_effects(state, effects).await?;

    Ok(TransactionResponse {
        gas_used,
        object_effects_summary: json!(object_effects_summary),
        certificate: json!(cert),
    })
}

pub fn sui_start(state: &mut ServerState) -> AsyncResult<HttpResponseOk<String>, HttpError> {
    Box::pin(async {
        if !state.authority_handles.is_empty() {
            return Err(custom_http_error(
                StatusCode::FORBIDDEN,
                String::from("Sui network is already running."),
            ));
        }

        let committee = Committee::new(
            state
                .config
                .authorities
                .iter()
                .map(|info| (*info.key_pair.public_key_bytes(), info.stake))
                .collect(),
        );
        let mut handles = FuturesUnordered::new();

        for authority in &state.config.authorities {
            let server = sui_commands::make_server(
                authority,
                &committee,
                vec![],
                &[],
                state.config.buffer_size,
            )
            .await
            .map_err(|error| {
                custom_http_error(
                    StatusCode::CONFLICT,
                    format!("Unable to make server: {error}"),
                )
            })?;
            handles.push(async move {
                match server.spawn().await {
                    Ok(server) => Ok(server),
                    Err(err) => {
                        return Err(custom_http_error(
                            StatusCode::FAILED_DEPENDENCY,
                            format!("Failed to start server: {}", err),
                        ));
                    }
                }
            })
        }

        let num_authorities = handles.len();
        info!("Started {} authorities", num_authorities);

        while let Some(spawned_server) = handles.next().await {
            state.authority_handles.push(task::spawn(async {
                if let Err(err) = spawned_server.unwrap().join().await {
                    error!("Server ended with an error: {}", err);
                }
            }));
        }

        for address in state.addresses.clone() {
            state
                .gateway
                .sync_account_state(address)
                .await
                .map_err(|err| {
                    custom_http_error(
                        StatusCode::FAILED_DEPENDENCY,
                        format!("Sync error: {:?}", err),
                    )
                })?;
        }
        Ok(HttpResponseOk(format!(
            "Started {} authorities",
            num_authorities
        )))
    })
}
