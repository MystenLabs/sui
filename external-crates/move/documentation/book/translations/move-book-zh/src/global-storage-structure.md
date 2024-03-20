# 全局存储 —— 结构

Move 程序的目的是[读取和写入](./global-storage-operators.md)树形的持久全局存储。程序不能访问文件系统、网络或任何此树以外的数据。

在伪代码中，全局存储看起来像：

```move
struct GlobalStorage {
  resources: Map<(address, ResourceType), ResourceValue>
  modules: Map<(address, ModuleName), ModuleBytecode>
}
```

从结构上讲，全局存储是一个[森林（forest）](https://en.wikipedia.org/wiki/Tree_(graph_theory))，这个森林由以账户[地址（`address`）](./address.md)为根的树组成。每个地址可以存储[资源（resource）](./structs-and-resources.md)数据和[模块（module）](./modules-and-scripts.md)代码。如上面的伪代码所示，每个地址（`address`）最多可以存储一个给定类型的资源值，最多可以存储一个给定名称的模块。
