// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests transferring the GasCoin by value when using --address-balance-gas.

//# init --addresses test=0x0 --accounts A --enable-address-balance-gas-payments --simulator

// Transfer gas coin to 0x0 via TransferObjects while paying with address balance
//# programmable --sender A --inputs @0x0 object(0,0) --address-balance-gas
//> TransferObjects([Gas], Input(0))
