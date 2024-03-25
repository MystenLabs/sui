# 对象显示（Object Display）

拥有 `Publisher` 对象的创作者或构建者可以使用 `sui::display` 模块来定义其对象的显示属性。请查看 [Publisher 页面](./publisher.md)关于获取 `Publisher` 对象的方法。

`Display<T>` 是一个为类型 `T` 指定了一组命名的模板的对象（例如，对于类型 `0x2::capy::Capy`，显示对象将是 `Display<0x2::capy::Capy>`）。所有类型为 `T` 的对象都将通过匹配的 `Display` 定义在 Sui 全节点 RPC 中进行处理，并在查询对象时附加已处理的结果。


<details>
<summary>English Version</summary>

A creator or a builder who owns a `Publisher` object can use the `sui::display` module to define display properties for their objects. To get a Publisher object check out [the Publisher page](./publisher.md).

`Display<T>` is an object that specifies a set of named templates for the type `T` (for example, for a type `0x2::capy::Capy` the display would be `Display<0x2::capy::Capy>`). All objects of the type `T` will be processed in the Sui Full Node RPC through the matching `Display` definition and will have processed result attached when an object is queried.

</details>

## 描述

Sui Object Display 是一个模板引擎，允许通过链上对类型显示进行配置以供生态系统在链下处理数据。它可以将模板中字符串替换为真实数据。

任意字段都可以被设置，对象的所有属性都可以通过 `{property}` 语法访问同时作为模板字符串的一部分插入其中（请参见示例）。

<details>
<summary>English Version</summary>

## Description

Sui Object Display is a template engine which allows for on-chain display configuration for type to be handled off-chain by the ecosystem. It has the ability to use an object's data for substitution into a template string.

There's no limitation to what fields can be set, all object properties can be accessed via the `{property}` syntax and inserted as a part of the template string (see examples for the illustration).

</details>

## 示例

对于以下 Hero 模块，Display 将根据类型 Hero 的 name、id 和 img_url 属性而变化。在 init 函数中定义的模板可以表示为：

```json
{
    "name": "{name}",
    "link": "https://sui-heroes.io/hero/{id}",
    "img_url": "ipfs://{img_url}",
    "description": "A true Hero of the Sui ecosystem!",
    "project_url": "https://sui-heroes.io",
    "creator": "Unknown Sui Fan"
}
```

<details>
<summary>English Version</summary>

## Example

For the following Hero module, the Display would vary based on the "name", "id" and "img_url" properties of the type "Hero". The template defined in the init function can be represented as:

```json
{
    "name": "{name}",
    "link": "https://sui-heroes.io/hero/{id}",
    "img_url": "ipfs://{img_url}",
    "description": "A true Hero of the Sui ecosystem!",
    "project_url": "https://sui-heroes.io",
    "creator": "Unknown Sui Fan"
}
```

</details>

```move
{{#include ../../examples_zh/sources/basics/display.move:4:}}
```

## 方法描述

Display 对象是通过 display::new<T> 调用创建的，可以在自定义函数（或模块初始化器）中执行，也作为可编程交易的一部分执行。

```move
module sui::display {
    /// Get a new Display object for the `T`.
    /// Publisher must be the publisher of the T, `from_package`
    /// check is performed.
    public fun new<T>(pub: &Publisher): Display<T> { /* ... */ }
}
```

一旦 Display 对象生成，可以通过以下方法更改：
```move
module sui::display {
    /// 同时更改多项内容
    /// Sets multiple fields at once
    public fun add_multiple(
        self: &mut Display,
        keys: vector<String>,
        values: vector<String
    ) { /* ... */ }

    /// 更改单项内容
    /// Edit a single field
    public fun edit(self: &mut Display, key: String, value: String) { /* ... */ }

    /// 从Display对象删除一个键值
    /// Remove a key from Display
    public fun remove(self: &mut Display, key: String ) { /* ... */ }
}
```

要使针对 Display 对象的更改生效需要调用 `update_version` 来触发一个事件（event），使得网络中各个完整节点监听到这个事件并获取类型 T 的新模板：

```move
module sui::display {
    /// 更新Display对象的模板，并产生一个事件
    /// Update the version of Display and emit an event
    public fun update_version(self: &mut Display) { /* ... */ }
}
```

<details>
<summary>English Version</summary>

## Methods description

Display is created via the `display::new<T>` call, which can be performed either in a custom function (or a module initializer) or as a part of a programmable transaction.

```move
module sui::display {
    /// Get a new Display object for the `T`.
    /// Publisher must be the publisher of the T, `from_package`
    /// check is performed.
    public fun new<T>(pub: &Publisher): Display<T> { /* ... */ }
}
```

Once acquired, the Display can be modified:
```move
module sui::display {
    /// Sets multiple fields at once
    public fun add_multiple(
        self: &mut Display,
        keys: vector<String>,
        values: vector<String
    ) { /* ... */ }

    /// Edit a single field
    public fun edit(self: &mut Display, key: String, value: String) { /* ... */ }

    /// Remove a key from Display
    public fun remove(self: &mut Display, key: String ) { /* ... */ }
}
```

To apply changes and set the Display for the T, one last call is required: `update_version` publishes version by emitting an event which Full Node listens to and uses to get a template for the type.
```move
module sui::display {
    /// Update the version of Display and emit an event
    public fun update_version(self: &mut Display) { /* ... */ }
}
```

</details>