// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// 在这个例子中我们将解释OTW如何工作
/// This example illustrates how One Time Witness works.
/// 一次性见证（OTW）是一个在整个系统中唯一的实例。它具有以下属性：
/// One Time Witness (OTW) is an instance of a type which is guaranteed to
/// be unique across the system. It has the following properties:
///
/// - created only in module initializer | 只可以在`init`函数中创建
/// - named after the module (uppercased) | 使用模块名命名（大写）
/// - cannot be packed manually | 无法手动构造
/// - has a `drop` ability | 拥有`drop`属性
module examples::one_time_witness_registry {
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};
    use std::string::String;
    use sui::transfer;

    // 通过使用`sui::types`检查对象类型是否为OTW
    // This dependency allows us to check whether type
    // is a one-time witness (OTW)
    use sui::types;

    /// 错误码0： 当传入对象非OTW时触发
    /// For when someone tries to send a non OTW struct
    const ENotOneTimeWitness: u64 = 0;

    /// 此类型的对象将标记存在一种类型，每种类型只能有一条记录。
    /// An object of this type will mark that there's a type,
    /// and there can be only one record per type.
    struct UniqueTypeRecord<phantom T> has key {
        id: UID,
        name: String
    }

    /// 提供一个公共函数用于注册`UniqueTypeRecord`对象
    /// `is_one_time_witness`将确保每个泛型（T）只可使用这个函数一次
    /// Expose a public function to allow registering new types with
    /// custom names. With a `is_one_time_witness` call we make sure
    /// that for a single `T` this function can be called only once.
    public fun add_record<T: drop>(
        witness: T,
        name: String,
        ctx: &mut TxContext
    ) {
        // 这里将检查传入值是否为OTW
        // This call allows us to check whether type is an OTW;
        assert!(types::is_one_time_witness(&witness), ENotOneTimeWitness);

        // :)
        // Share the record for the world to see. :)
        transfer::share_object(UniqueTypeRecord<T> {
            id: object::new(ctx),
            name
        });
    }
}

/// 创建一个OTW的例子
/// Example of spawning an OTW.
module examples::my_otw {
    use std::string;
    use sui::tx_context::TxContext;
    use examples::one_time_witness_registry as registry;

    /// 使用模块名命名但是全部大写
    /// Type is named after the module but uppercased
    struct MY_OTW has drop {}

    /// 通过`init`函数的一个参数获取`MY_OTW`, 注意这并不是一个引用类型
    /// To get it, use the first argument of the module initializer.
    /// It is a full instance and not a reference type.
    fun init(witness: MY_OTW, ctx: &mut TxContext) {
        registry::add_record(
            witness, // here it goes <= 在这里使用
            string::utf8(b"My awesome record"),
            ctx
        )
    }
}
