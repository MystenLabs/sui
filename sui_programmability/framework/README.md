# Sui Programmability with Move

This is a proof-of-concept Move standard library for Sui (`sources/`), along with several examples of programs that Sui users might want to write (`examples`). `CustomObjectTemplate.move` is a good starting point for understanding the proposed model.

### Setup

```
# install Move CLI
cargo install --git https://github.com/diem/diem move-cli --branch main
# put it in your PATH
export PATH="$PATH:~/.cargo/bin"
```

For reading/editing Move, your best bet is vscode + this [plugin](https://marketplace.visualstudio.com/items?itemName=move.move-analyzer).

### Building

```
# Inside the sui_programmability/framework dir
move package -d build
```
