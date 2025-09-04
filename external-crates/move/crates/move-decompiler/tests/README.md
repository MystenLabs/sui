# Running tests

```bash
cargo test -vv -p revela
```

# Generating missing -decompiled files

```bash
UPDATE_EXPECTED_OUTPUT=1 cargo test -p revela
```

or generate all -decompiled files


```bash
FORCE_UPDATE_EXPECTED_OUTPUT=1 cargo test -p revela
```
