// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests TransferObject should fail for a package

//# init --accounts A B --addresses test=0x0

//# publish --sender A

module test::m {}


//# view-object 105

//# transfer-object 105 --sender A --recipient B

//# view-object 105
