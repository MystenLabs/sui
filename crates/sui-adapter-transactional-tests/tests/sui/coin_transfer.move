// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test basic coin transfer

//# init --accounts A B C

//# view-object 101

//# run sui::pay::split_and_transfer --type-args sui::sui::SUI --args object(101) 10 @B --sender A

//# view-object 101

//# view-object 107

//# run sui::pay::split_and_transfer --type-args sui::sui::SUI --args object(101) 0 @C --sender B

//# view-object 101
