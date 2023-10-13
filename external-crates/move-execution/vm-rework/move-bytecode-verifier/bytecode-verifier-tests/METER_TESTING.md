This testsuite can be run in a specific way to print the time until a 'complex' program is detected or accepted. Call as in:

```
cargo test --release --features=address32   -- --nocapture 1>/dev/null
```
