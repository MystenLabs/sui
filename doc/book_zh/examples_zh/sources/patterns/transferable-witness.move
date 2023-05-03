// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// 可转移见证模式（transferable witness）基于权限凭证（Capability）和见证（Witness）模式。
/// 因为Witness需要小心处理，因此只有经过授权的用户（理想情况下只使用一次）才能生成它。
/// 但是，在某些情况中需要模块X对类型进行授权以便在另一个模块Y中使用，或者需要一段时间后使用。
/// This pattern is based on a combination of two others: Capability and a Witness.
/// Since Witness is something to be careful with, spawning it should be allowed
/// only to authorized users (ideally only once). But some scenarios require
/// type authorization by module X to be used in another module Y. Or, possibly,
/// there's a case where authorization should be performed after some time.
///
/// 在这些特殊的场景下，可存储的见证（storable witness）就排上了用场。
/// For these rather rare scerarios, a storable witness is a perfect solution.
module examples::transferable_witness {
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};

    /// Witness具有store属性因此可以将其存储在包装结构体中
    /// Witness now has a `store` that allows us to store it inside a wrapper.
    struct WITNESS has store, drop {}

    /// `WitnessCarrier`是`WITNESS`的封装容器，只能获得一次`WITNESS`
    /// Carries the witness type. Can be used only once to get a Witness.
    struct WitnessCarrier has key { id: UID, witness: WITNESS }

    /// 将`WitnessCarrier`发送给模块的发布者
    /// Send a `WitnessCarrier` to the module publisher.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(
            WitnessCarrier { id: object::new(ctx), witness: WITNESS {} },
            tx_context::sender(ctx)
        )
    }

    /// 解开包装获得`WITNESS`
    /// Unwrap a carrier and get the inner WITNESS type.
    public fun get_witness(carrier: WitnessCarrier): WITNESS {
        let WitnessCarrier { id, witness } = carrier;
        object::delete(id);
        witness
    }
}
