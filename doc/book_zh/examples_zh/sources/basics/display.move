// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// 示例： "Sui Hero"藏品集
/// 任何人都可以创建属于自己的`Hero`， 在这个案例中我们将展示如何初始化`Publisher`,
/// 如何使用`Publisher`获取`Display<Hero>`对象--在生态系统中表示某一类型。
/// Example of an unlimited "Sui Hero" collection - anyone is free to
/// mint their Hero. Shows how to initialize the `Publisher` and how
/// to use it to get the `Display<Hero>` object - a way to describe a
/// type for the ecosystem.
module examples::my_hero {
    use sui::tx_context::{sender, TxContext};
    use std::string::{utf8, String};
    use sui::transfer;
    use sui::object::{Self, UID};

    // 创造者捆绑包：这两个包通常一起使用
    // The creator bundle: these two packages often go together.
    use sui::package;
    use sui::display;

    /// Hero结构体 - 用以代表数字藏品
    /// The Hero - an outstanding collection of digital art.
    struct Hero has key, store {
        id: UID,
        name: String,
        img_url: String,
    }

    /// 当前模块的 OTW
    /// One-Time-Witness for the module.
    struct MY_HERO has drop {}

    /// 在模块初始化函数中我们声明`Publisher`对象然后创建`Display`对象。
    /// `Display`将在初始化时设置多个项（之后可以更改），使用`update_version`发布。
    /// In the module initializer we claim the `Publisher` object
    /// to then create a `Display`. The `Display` is initialized with
    /// a set of fields (but can be modified later) and published via
    /// the `update_version` call.
    /// 
    /// `Display`对象的键值对可以在初始化时设置也可以在对象创建后更改
    /// Keys and values are set in the initializer but could also be
    /// set after publishing if a `Publisher` object was created.
    fun init(otw: MY_HERO, ctx: &mut TxContext) {
        let keys = vector[
            utf8(b"name"),
            utf8(b"link"),
            utf8(b"image_url"),
            utf8(b"description"),
            utf8(b"project_url"),
            utf8(b"creator"),
        ];

        let values = vector[
            // `name`对应`Hero.name`的值
            // For `name` we can use the `Hero.name` property
            utf8(b"{name}"),
            // `link`对应包括`Hero.id`的链接
            // For `link` we can build a URL using an `id` property
            utf8(b"https://sui-heroes.io/hero/{id}"),
            // `img_url`使用IPFS链接的模版
            // For `img_url` we use an IPFS template.
            utf8(b"ipfs://{img_url}"),
            // 一个针对所有`Hero`对象的描述
            // Description is static for all `Hero` objects.
            utf8(b"A true Hero of the Sui ecosystem!"),
            // 一个针对所有`Hero`藏品的网站链接
            // Project URL is usually static
            utf8(b"https://sui-heroes.io"),
            // 一个任意的项
            // Creator field can be any
            utf8(b"Unknown Sui Fan")
        ];

        // 为整个包创建`Publisher`对象
        // Claim the `Publisher` for the package!
        let publisher = package::claim(otw, ctx);

        // 为`Hero`类型创建`Display` 对象
        // Get a new `Display` object for the `Hero` type.
        let display = display::new_with_fields<Hero>(
            &publisher, keys, values, ctx
        );

        // 提交第一个版本`Display`
        // Commit first version of `Display` to apply changes.
        display::update_version(&mut display);

        transfer::public_transfer(publisher, sender(ctx));
        transfer::public_transfer(display, sender(ctx));
    }

    /// 任何人都可以创建`Hero`
    /// Anyone can mint their `Hero`!
    public fun mint(name: String, img_url: String, ctx: &mut TxContext): Hero {
        let id = object::new(ctx);
        Hero { id, name, img_url }
    }
}
