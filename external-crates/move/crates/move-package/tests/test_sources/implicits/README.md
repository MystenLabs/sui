Tests for implicit dependency resolution
========================================

Notation: we use capital letters for names of deps, and lower case letters for
their implementations. That is:

```
a:
  I1 = i1
```

is shorthand for a `Move.toml` in directory `a` containing `I1 = { local = "../i1" }`

In the following, all tests have the following implicit dependencies:

```
i1:
 I2 = i2

i2: no deps
```

Expected output graphs are oriented left to right

Tests
-----


### Simple

implicit deps should be added

```
a: no deps
```

Expected output:

```
a ── i1 ──┐
└──────── i2
```


### Override 1

```
a:
 I1 = b

b: no deps
```

no implicit deps should not be added since 

```
a ── b
```

