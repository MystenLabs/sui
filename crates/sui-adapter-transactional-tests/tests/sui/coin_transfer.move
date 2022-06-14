// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test basic coin transfer

//# init --accounts A B C

//# view-object 100

//# run sui::Coin::split_and_transfer --type-args sui::SUI::SUI --args object(100) 10 @B --sender A

//# view-object 100

//# view-object 106

//# run sui::Coin::transfer --type-args sui::SUI::SUI --args object(100) @C --sender B

//# view-object 100

//# view-object 107
