---
"@mysten/bcs": minor
---

Fixes the issue with deep nested generics by introducing array type names

- all of the methods (except for aliasing) now allow passing in arrays instead
  of strings to allow for easier composition of generics and avoid using template
  strings

```js
// new syntax
bcs.registerStructType(["VecMap", "K", "V"], {
  keys: ["vector", "K"],
  values: ["vector", "V"],
});

// is identical to an old string definition
bcs.registerStructType("VecMap<K, V>", {
  keys: "vector<K>",
  values: "vector<V>",
});
```

Similar approach applies to `bcs.ser()` and `bcs.de()` as well as to other register\* methods
