// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO/XXX(vm-rewrite): This test is currently disabled because this behavior is not protocol gated in the VM.
// Once we make the execution version cut for the new and old VM, this test should be re-enabled and re-generated.

//# init --addresses test=0x0 --accounts A --protocol-version 71

// //# programmable --sender A 
// //> 0: MakeMoveVec<std::string::utf8>([]);
// 
// //# programmable --sender A --inputs 1
// //> 0: MakeMoveVec<std::string::utf8>([Input(0)]);
