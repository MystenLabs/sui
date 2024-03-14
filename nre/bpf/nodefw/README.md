# nodefw

## Prerequisites Linux x86_64

1. Install bpf-linker: `cargo install bpf-linker`

## Prerequisites For Any Other Architecture

1. Install bpf-linker: `cargo install --no-default-features bpf-linker`

## Template Generation
You can optionally use templates to generate a new project. (Recommended)

`cargo install cargo-generate`

## Starting A New Project
Although nodefw is already here, if you want to switch to a new directory and make
another filter...

`cargo generate https://github.com/aya-rs/aya-template`

NB some templates contain minor issues due to code drift. They do keep them updated
but sometimes fall behind.

## Build eBPF

```bash
cargo bpf build-ebpf
```

To perform a release build you can use the `--release` flag.
You may also change the target architecture with the `--target` flag.

## Build Userspace

```bash
cargo build
```

## Run
NB the RUST_LOG env var. if you wish to run the binary outside of a cargo run, you'll need linux capabilites.  xtask does `sudo -E` for you automatically, but if you don't you'll need to set
capabilites on the built binary.  Some or all of these, depending on the program.

`CAP_SYS_ADMIN`
`CAP_NET_ADMIN`
`CAP_BPF`

Alternatively, run with `sudo -E`

```bash
RUST_LOG=info cargo bpf run
```
