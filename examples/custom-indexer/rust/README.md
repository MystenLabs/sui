# Sui Custom Indexer Example
This is a complimentary example to the Sui Custom Indexer documentation.
It demonstrates how to create a custom indexer for the Sui search engine.
See the [Sui Custom Indexer documentation](https://docs.sui.io/guides/developer/advanced/custom-indexer) for more information.

## Prerequisites
- Rust

## How to install
Once you have Rust installed, you can build the custom indexer by running the following command:
```bash
cargo build
```

## How to run
### Remote Reader example
```sh
cargo run --bin remote_reader
```

### Local Reader example
The local reader example saves progress in a file called `/tmp/local_reader_progress` and monitors checkpoint files in the `chk` directory


To test the local reader example, create the `/tmp/local_reader_progress` file first
```sh
echo "{\"local_reader\": 1}" > /tmp/local_reader_progress
```

then, create the `chk` directory in the same level as the `local_reader.rs` file
```sh
mkdir -p chk
```

then, run the local reader example
```sh
cargo run --bin local_reader
```

Finally, copy the checkpoint files to the `chk` directory and the program should process the checkpoint files as they come in
```sh
cp $YOUR_CHECKPOINT_FILE chk/
```
