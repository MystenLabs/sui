# `Move.lock` file for Move packages

## What is it and what do I do with it?

The `Move.lock` file is automatically created when you build a `Move` package. It contains data about your package (like dependencies). This aids operations like verifying your source code against on-chain packages and ensures compatibility with package managers.

Do: check in the generated `Move.lock` file if you use source control. It will be created in your package root (where `Move.toml` is).

Don't: Manually edit the `Move.lock` file by hand, or move it to another directory.

A full technical description of the [original design is available](https://docs.google.com/document/d/1OV3te-SnpZv2Yxv7uxGQH6NFhE-CdqiCjB66JmYAGKs/edit#heading=h.byj11m1l42gu), which describes the schema, including headers like `[dependencies]` found in the `Move.lock` file.
