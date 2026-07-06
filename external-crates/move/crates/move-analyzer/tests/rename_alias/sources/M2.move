// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module RenameAlias::M2 {
    use RenameAlias::M1::{MyStruct as Data, create as make};

    public fun alias_use(): Data {
        let d: Data = make();
        d
    }

    public fun direct_use(): RenameAlias::M1::MyStruct {
        RenameAlias::M1::create()
    }

    public fun mixed(x: Data): RenameAlias::M1::MyStruct {
        x
    }
}
