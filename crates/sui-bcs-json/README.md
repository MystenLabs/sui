# Description

This library provides an interface for transforming BCS data into JSON and vice-versa. The aim is to provide a way to support BCS in an interoperable manner for languages that do not have native BCS support, but have C interoperability.

# Design choices

The library was designed to be as simple as possible to allow users to convert to JSON or to BCS by passing the `typename` (as BCS is schema-less so requires type information to be supplied during deserialization). Specifically, the types that are supported to be converted as of now are all the primitives (`u8,u16,u32,u64, u128, u256`, `string`), `SuiAddress`, `MultiSig`, `MultiSigPublicKey`, `TransactionData`, and `TransactionKind` (the last two can be found in [transaction.rs](https://github.com/MystenLabs/sui/blob/main/crates/sui-types/src/transaction.rs).

## API
The library offers two functions for serializing and deserializing data:
* **sui_json_to_bcs**
```c
size_t sui_json_to_bcs(const char *type_name,
                         const char *json_data,
                         const uint8_t **result,
                         size_t *length);
```

This function **sui_json_to_bcs** -- will transfrom a JSON string input into a BCS array (u8). The function takes as input a typename (`type_name`), the input json string (`json_data`), a pointer to a pointer where to store the results (`result`), and a pointer to store the length of the result. The function returns 0 for success, and 1 for error (e.g., cannot deserialize the JSON). The exact error message can be accessed through a special function, see [Errors](#Error) below.


* **sui_bcs_to_json**

```c
size_t sui_bcs_to_json(const char *type_name,
                       const uint8_t **bcs_buf_ptr,
                       size_t len,
                       const char **result,
                       bool pretty);
  
```


This function **sui_bcs_to_json** will transform a BCS array input into a JSON string. The function takes as input a typename (`type_name`), the input BCS array (`bcs_buf_ptr`) as a pointer, a pointer to the length of the result (`length`), and a boolean flag if the JSON output should be pretty printed or not (`pretty`). 
It will return 0 if the conversion from BCS to JSON is successful, and 1 or 2 for failure. 1 represents a failure from parsing the BCS to JSON, and 2 represents an error building the CString from the JSON data. The exact error message can be accessed through a special function, see [Errors](#Error) below.


## Memory management
There are two functions that should be used to free Rust allocated data (`Vec<u8>`` and `String`):

```c
/**
 * Frees a Rust-allocated Vec<u8>.
 */
void sui_bcs_json_free_array(const uint8_t *ptr, size_t len);

/**
 * Frees a Rust-allocated string.
 */
void sui_bcs_json_free_string(const char *pointer);
```

## Errors
The library uses two return error codes to indicate an error occurred: 1 or 2. A return code of 0 indicates that the function call was successful. 
In addition to the error code, an actual string error message is set everytime there is an error, and it can be accessed by calling the `sui_last_error_message_utf8` function and passing a pointer to a buffer and the length of the error message that should be retrieved by calling `sui_last_error_length` function. See the `sui_bcs_json_bindings.h` file for more information.

Please note that the error messages are encoded as UTF-8, and any transformation to other formats needs to be done by the callee.

# How to run the example

The library offers a basic example written in C, that calls the the main two functions `sui_bcs_to_json` and `sui_bcs_from_json` with dummy data and prints the output. To run the example, run:
* Cargo: `cargo test && cargo build && `
* `make main && ./examples/main`

If you do not have `make` installed, run `cargo build && cc examples/main.c -o examples/main -L target/debug -l sui_bcs_json -l{pthread,dl,m}` to compile, and then `./example/main` to run the binary.

# Make rules
* make bindings --> will generate the `sui_bcs_json_bindings.h` file after running tests
* make example --> will compile `examples/main.c` into an executable `example/main.o`
* make rustlib --> will call `cargo build` to compile the library. See the output in the target/debug folder, including the library files
* make main --> will compile `main.c`, after running tests, build, and generating bindings
* ./examples/main to run the example C program
