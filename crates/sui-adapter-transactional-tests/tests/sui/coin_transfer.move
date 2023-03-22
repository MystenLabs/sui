// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test basic coin transfer

//# init --accounts A B C

//# view-object 104

//# run sui::pay::split_and_transfer --type-args sui::sui::SUI --args object(104) 10 @A --sender B

//# view-object 104

//# view-object 107

//# run sui::pay::split_and_transfer --type-args sui::sui::SUI --args object(104) 0 @C --sender A

//# view-object 104
