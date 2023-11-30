// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "sui_bcs_json_bindings.h"

int main (int argc, char const * const argv[])
{

    // Fail on purpose and retrieve error message
    char *json_str_0 = "{\"Pure\": [49,50,51] }";
    const uint8_t *json_result_0;
    size_t result_len_0;
    int result_0 = sui_json_to_bcs("Test", json_str_0, &json_result_0 , &result_len_0);

    if (result_0 == 1) {
        printf("Error code is %d. Error mesasge is: ", result_0);
        int buffer_len = sui_last_error_length();
        char *error_msg = malloc(buffer_len);
        sui_last_error_message_utf8(error_msg, buffer_len);
        printf("%s\n", error_msg);
        free(error_msg);
    }

    // Transform BCS representing a Pure value into JSON
    char const *result;
    uint8_t bcs[] = { 0, 3, 49, 50, 51 };
    int r = sui_bcs_to_json("call_arg", bcs, sizeof(bcs), &result, true);
    printf("JSON string from BCS [0,3,49,50,51] of type CallArg is:\n%s\n", result);
    sui_bcs_json_free((uint8_t*)result, strlen(result));

    // Transform a JSON to a BCS array
    char *json_str = "{\"Pure\": [49,50,51] }";
    const uint8_t *json_result;
    size_t result_len;
    int r_json = sui_json_to_bcs("call_arg", json_str, &json_result, &result_len);
    printf("Input string is: %s. Expected BCS output is [0,3,49,50,51]\n", json_str);
    printf("BCS output: ");
    for (size_t i=0; i<result_len; i++) {
        printf("%d,", json_result[i]);
    }
    printf("\n");
    sui_bcs_json_free(json_result, result_len);
        
    return EXIT_SUCCESS;
}
