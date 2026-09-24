// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Package policy applies to exact declared dependencies of publishes and upgrades, including when
// those commands execute an initializer.

//# init --addresses Leaf=0x0 LeafV2=0x0 MinDependencyV1=0x0 MinDependencyV2=0x0 Publisher=0x0 ForbiddenPublisher=0x0 ForbiddenUpgradeV1=0x0 ForbiddenUpgradeV2=0x0 UpgradePublisherV1=0x0 UpgradePublisherV2=0x0 --accounts A

//# publish --upgradeable --sender A
module Leaf::leaf;

public fun ping() {}

//# upgrade --package Leaf --upgrade-capability 1,1 --sender A
module LeafV2::leaf;

public fun ping() {}

//# publish --upgradeable --sender A
module MinDependencyV1::dependency;

public fun ping() {}

//# upgrade --package MinDependencyV1 --upgrade-capability 3,1 --sender A
module MinDependencyV2::dependency;

public fun ping() {}

//# publish --upgradeable --dependencies Leaf --sender A
module ForbiddenUpgradeV1::publisher;
use Leaf::leaf;

public fun publish_time_only() { leaf::ping() }

//# run sui::package_config::forbid_version --args object(0x426) object(1,1) 1 --sender A

//# publish --upgradeable --dependencies MinDependencyV1 --sender A
module UpgradePublisherV1::publisher;
use MinDependencyV1::dependency;

public fun publish_time_only() { dependency::ping() }

//# programmable --sender A --inputs object(3,1) object(0x426)
//> 0: sui::package::enable_minversion(Input(0));
//> sui::package_config::record_minversion_enrollment(Input(1), Result(0));

// This package has no init. Its MinDependencyV1 dependency is below minversion, which selects
// MinDependencyV2, so it is rejected.
//# publish --upgradeable --dependencies MinDependencyV1 --sender A
module UpgradePublisherV1::publisher;
use MinDependencyV1::dependency;

public fun publish_time_only() { dependency::ping() }

// This publish executes init and its declared dependency is below minversion.
//# publish --dependencies MinDependencyV1 --sender A
module Publisher::publisher;
use MinDependencyV1::dependency;

fun init(_ctx: &mut sui::tx_context::TxContext) { }
public fun publish_time_only() { dependency::ping() }

// This publish executes init and its declared dependency is forbidden.
//# publish --dependencies Leaf --sender A
module ForbiddenPublisher::publisher;
use Leaf::leaf;

fun init(_ctx: &mut sui::tx_context::TxContext) { }
public fun publish_time_only() { leaf::ping() }

// This upgrade introduces a new module with init and has a below-minversion dependency.
//# upgrade --package UpgradePublisherV1 --upgrade-capability 7,1 --dependencies MinDependencyV1 --sender A
module UpgradePublisherV2::publisher {
    use MinDependencyV1::dependency;

    public fun publish_time_only() { dependency::ping() }
}
module UpgradePublisherV2::init_module {
    fun init(_ctx: &mut sui::tx_context::TxContext) { }
}

// This upgrade also introduces a new module with init, but its direct dependency is forbidden.
//# upgrade --package ForbiddenUpgradeV1 --upgrade-capability 5,1 --dependencies Leaf --sender A
module ForbiddenUpgradeV2::publisher {
    use Leaf::leaf;

    public fun publish_time_only() { leaf::ping() }
}
module ForbiddenUpgradeV2::init_module {
    fun init(_ctx: &mut sui::tx_context::TxContext) { }
}
