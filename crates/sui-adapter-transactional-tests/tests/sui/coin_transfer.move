// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test basic coin transfer

//# init --accounts A B

//# view-object 100

//# run Sui::Coin::transfer_ --type-args Sui::SUI::SUI --args object(100) 10 @B

//# view-object 100

//# view-object 105
