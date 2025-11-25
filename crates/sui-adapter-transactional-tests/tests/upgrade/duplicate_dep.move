// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses TestV1=0x0 TestV2=0x0 DepV1=0x0 --accounts A

//# publish --upgradeable --sender A
module DepV1::dep;

//# publish --upgradeable --dependencies DepV1 DepV1 --sender A
module TestV1::m;

//# upgrade --package TestV1 --upgrade-capability 2,1 --dependencies DepV1 DepV1 --sender A
module TestV2::m;
