# 入口函数（Entry Functions）

入口函数（entry function）修饰符用于可以直接被调用的函数，例如在交易（transaction）中直接调用。它可以和其它的函数可见性修饰符配合使用， 例如 `public`, 这样可以使函数被其他模块（module）调用， 或者配合 `public(entry)` 使用, 使得函数可以被 `friend` 模块调用。

<details>
<summary>English Version</summary>

An [entry function](https://docs.sui.io/build/move#entry-functions) visibility modifier allows a function to be called directly (eg in transaction). It is combinable with other visibility modifiers, such as `public` which allows calling from other modules) and `public(friend)` for calling from *friend* modules.

</details>


```move
{{#include ../../examples_zh/sources/basics/entry-functions.move:4:}}
```
