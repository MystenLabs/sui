# Troubleshooting

## Sui Framework change

If Sui framework code got updated, the expectations need to be changed. Follow these steps:

```bash
# required; can be omitted if cargo-insta is installed
$ cargo install cargo-insta

# run in ./sui-cost
$ cargo insta test --review
```
