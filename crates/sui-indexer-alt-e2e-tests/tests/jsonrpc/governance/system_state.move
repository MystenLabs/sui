// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --simulator --accounts A

//# run-jsonrpc
{
  "method": "suix_getLatestSuiSystemState",
  "params": []
}

//# programmable --sender A --inputs 1000000000 object(0x5) @validator_0
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui_system::sui_system::request_add_stake(Input(1), Result(0), Input(2))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getLatestSuiSystemState",
  "params": []
}

//# advance-clock --duration-ns 1000000

//# advance-epoch

//# run-jsonrpc
{
  "method": "suix_getLatestSuiSystemState",
  "params": []
}

//# programmable --sender A --inputs object(0x5) object(2,1)
//> 0: sui_system::sui_system::request_withdraw_stake(Input(0), Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getLatestSuiSystemState",
  "params": []
}

//# advance-epoch

//# run-jsonrpc
{
  "method": "suix_getLatestSuiSystemState",
  "params": []
}
