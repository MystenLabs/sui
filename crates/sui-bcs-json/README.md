# Description

This library provides an interface for transforming BCS data into JSON and vice-versa. The aim is to provide a way to support BCS in an interoperable manner for languages that do not have native BCS support, but have C interoperability.

# Design choices

The library was designed to be as simple as possible to allow users to convert BCS to JSON and vice-versa by passing the `typename` (as BCS is schema-less so requires type information to be supplied during deserialization). Specifically, the types that are supported to be converted as of now are all the primitives (`u8, u16, u32, u64, u128, u256`, `string`), `SuiAddress`, `MultiSig`, `MultiSigPublicKey`, `TransactionData`, and `TransactionKind` (the last two can be found in [transaction.rs](https://github.com/MystenLabs/sui/blob/main/crates/sui-types/src/transaction.rs).

## API
The library offers two functions for serializing and deserializing data:
* **sui_json_to_bcs**
```c
size_t sui_json_to_bcs(const char *type_name,
                       const char *json_data,
                       const uint8_t **result,
                       size_t *length);
```

This function converts a JSON string input into a BCS array (u8). The function takes as input a typename (`type_name`), the input json string (`json_data`), a pointer to a pointer where to store the results (`result`), and a pointer to store the length of the result (`length`). The function returns 0 for success, and 1 for error (e.g., cannot deserialize the JSON). The exact error message can be accessed through a special function, see [Errors](#Error) below.

* **sui_bcs_to_json**

```c
size_t sui_bcs_to_json(const char *type_name,
                       const uint8_t *bcs_buf_ptr,
                       size_t len,
                       const char **result,
                       bool pretty);
  
```


This function converts a BCS array into a JSON string. The function takes as input a typename (`type_name`), a pointer to the input BCS array (`bcs_buf_ptr`), a pointer for the length of the result (`length`), a pointer to a pointer where to store the result's value (`result`), and a boolean flag if the JSON output should be pretty printed or not (`pretty`). 
It will return 0 if the conversion from BCS to JSON is successful, and 1 or 2 for failure. 1 represents a failure from parsing the BCS to JSON, and 2 represents an error building the CString from the JSON data. The exact error message can be accessed through a special function, see [Errors](#Error) below.


## Memory management
To deallocate Rust allocated data, use the following function:

```c
/**
 * Frees a Rust-allocated Vec<u8>.
 */
void sui_bcs_json_free(const uint8_t *ptr, size_t len);
```

## Errors
The library returns code 0 for a successful call, and 1 or 2 if an error occurred:
* 1 for failing to create the Rust strings from the input pointers
* 2 for failing to convert the BCS array into JSON (`sui_bcs_to_json`) or JSON to BCS (`sui_json_to_bcs`).

In addition to the error code, an actual string error message is set everytime there is an error. The following two functions should be used to retrieve the error message:
* `sui_last_error_length` returns the size of the error message, which is needed for the next function,
* `sui_last_error_message_utf8` takes as input a pointer to a buffer and the length of the error message (which should be passed from the function above). It will store the error message into the given buffer.

See the `sui_bcs_json_bindings.h` file for the exact function signatures.

Please note that the error messages are encoded as UTF-8, and any transformation to other formats needs to be done by the callee.

# How to run the example

The library offers a basic example written in C, that calls the the main two functions `sui_bcs_to_json` and `sui_json_to_bcs` with dummy data and prints the output. To run the example, run:
* Cargo: `cargo test && cargo build && `
* `make main && ./examples/main`

If you do not have `make` installed, run `cargo build && cc examples/main.c -o examples/main -L target/debug -l sui_bcs_json -l{pthread,dl,m}` to compile, and then `./examples/main` to run the binary.

# Make rules
* `make bindings` --> will generate the `sui_bcs_json_bindings.h` file after running tests, and will copy it to the `examples` folder.
* `make example` --> will compile `examples/main.c` into an executable `examples/main.o`.
* `make rustlib` --> will call `cargo build --target-dir target` to compile the library. See the output in the `target/debug` folder, including the library files.
* `make main` --> will compile `main.c`, after running tests, build, and generating bindings.
* `./examples/main` to run the example C program.
