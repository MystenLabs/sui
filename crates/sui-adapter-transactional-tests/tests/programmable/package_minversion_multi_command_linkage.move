// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Unified linkage must be independent of command order. Historical calls into two enrolled package
// families both select their v2 packages. Two entry consumers are individually valid with exact
// LeafV2 and LeafV3 closures, but cannot share one PTB because exact constraints at different leaf
// versions conflict. Both command orders exercise the same outcomes.

//# init --addresses LeafV1=0x0 LeafV2=0x0 LeafV3=0x0 AV1=0x0 AV2=0x0 BV1=0x0 BV2=0x0 ConsumerV2=0x0 ConsumerV3=0x0 --accounts A

//# publish --upgradeable --sender A
module LeafV1::leaf;
public fun value(): u64 { 1 }

//# upgrade --package LeafV1 --upgrade-capability 1,1 --sender A
module LeafV2::leaf;
public fun value(): u64 { 2 }

//# upgrade --package LeafV2 --upgrade-capability 1,1 --sender A
module LeafV3::leaf;
public fun value(): u64 { 3 }

//# publish --upgradeable --dependencies LeafV1 --sender A
module AV1::a;
use LeafV1::leaf;
public struct Selected has copy, drop { package: u64, leaf: u64 }
public fun call() { sui::event::emit(Selected { package: 1, leaf: leaf::value() }) }

//# upgrade --package AV1 --upgrade-capability 4,1 --dependencies LeafV2 --sender A
module AV2::a;
use LeafV2::leaf;
public struct Selected has copy, drop { package: u64, leaf: u64 }
public fun call() { sui::event::emit(Selected { package: 2, leaf: leaf::value() }) }

//# publish --upgradeable --dependencies LeafV1 --sender A
module BV1::b;
use LeafV1::leaf;
public struct Selected has copy, drop { package: u64, leaf: u64 }
public fun call() { sui::event::emit(Selected { package: 1, leaf: leaf::value() }) }

//# upgrade --package BV1 --upgrade-capability 6,1 --dependencies LeafV2 --sender A
module BV2::b;
use LeafV2::leaf;
public struct Selected has copy, drop { package: u64, leaf: u64 }
public fun call() { sui::event::emit(Selected { package: 2, leaf: leaf::value() }) }

//# publish --dependencies AV1 LeafV2 --sender A
module ConsumerV2::consumer;
use AV1::a;
entry fun call() { a::call() }

//# publish --dependencies BV1 LeafV3 --sender A
module ConsumerV3::consumer;
use BV1::b;
entry fun call() { b::call() }

//# programmable --sender A --inputs object(4,1) object(6,1) object(0xcf6)
//> 0: sui::package::enable_minversion_permanently(Input(0));
//> 1: sui::package_config::record_minversion_enrollment(Input(2), Result(0));
//> 2: sui::package::enable_minversion_permanently(Input(1));
//> sui::package_config::record_minversion_enrollment(Input(2), Result(2));

// Both historical package calls select v2 and execute against LeafV2.
//# programmable --sender A
//> 0: AV1::a::call();
//> BV1::b::call();

//# programmable --sender A
//> 0: BV1::b::call();
//> AV1::a::call();

// Each entry consumer succeeds alone: its exact leaf version satisfies the selected package's
// minimum LeafV2 dependency.
//# run ConsumerV2::consumer::call --sender A

//# run ConsumerV3::consumer::call --sender A

// Together, exact LeafV2 and exact LeafV3 cannot be unified, in either command order.
//# programmable --sender A
//> 0: ConsumerV2::consumer::call();
//> ConsumerV3::consumer::call();

//# programmable --sender A
//> 0: ConsumerV3::consumer::call();
//> ConsumerV2::consumer::call();
