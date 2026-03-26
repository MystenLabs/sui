// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests valid gas coin usage by value in protocol version 118, where send_funds is not yet enabled
// for gas coin usage

//# init --addresses test=0x0 --accounts A B --protocol-version 118

//# programmable --sender A --inputs @B
//> TransferObjects([Gas], Input(0))

//# view-object 0,0

//# programmable --sender B --inputs @A --gas-payment object(0,0)
// This should fai:
//> sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0))
