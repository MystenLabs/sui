// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses v0=0x0 v1=0x0 v2=0x0 --accounts A

//# publish --upgradeable --sender A
module v0::m {
}

//# upgrade --package v0 --upgrade-capability 1,1 --sender A
module v1::m {
}
module v1::n {
}

//# upgrade --package v1 --upgrade-capability 1,1 --sender A
module v2::m {
}
