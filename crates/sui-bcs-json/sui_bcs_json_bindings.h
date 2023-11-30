#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Converts the JSON data into a BCS array.
 * The result points to the address where the new BCS
 * array is stored. Don't forget to deallocate the memory
 * by calling the sui_bcs_json_free_array function.
 *
 * Returns 0 for success, 1 for failing to create the Rust strings from the
 * input pointers and 2 for failing to convert the JSON to BCS.
 *
 * # Safety
 * Unsafe function.
 */
size_t sui_json_to_bcs(const char *type_name,
                       const char *json_data,
                       const uint8_t **result,
                       size_t *length);

/**
 * Converts the BCS array into a JSON string.
 * The result argument will point to the address where the JSON
 * string is stored. Make sure you release the allocated memory
 * by calling sui_bcs_json_free_string function!
 *
 * Returns 0 for success, 1 for failing to create the Rust strings from the
 * input pointers and 2 for failing to convert the BCS array into JSON.
 *
 * # Safety
 * Unsafe function.
 */
size_t sui_bcs_to_json(const char *type_name,
                       const uint8_t *bcs_ptr,
                       size_t len,
                       const char **result,
                       bool pretty);

/**
 * Free Rust-allocated memory.
 */
void sui_bcs_json_free(const uint8_t *ptr, size_t len);

/**
 * Get the length of the last error message in bytes when encoded as UTF-8, including the trailing null. This function wraps last_error_length from ffi_helpers crate.
 */
int sui_last_error_length(void);

/**
 * Peek at the most recent error and write its error message (Display impl) into the provided buffer as a UTF-8 encoded string.
 *
 * This returns the number of bytes written, or -1 if there was an error.
 * This function wraps error_message_utf8 function from ffi_helpers crate.
 *
 * # Safety
 * This is an unsafe function
 */
int sui_last_error_message_utf8(char *buffer,
                                int length);

/**
 * Clear the last error message
 *
 * # Safety
 * This is an unsafe function
 */
void sui_clear_last_error_message(void);
