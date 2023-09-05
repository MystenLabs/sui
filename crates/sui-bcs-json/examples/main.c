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
    int result_0 = sui_bcs_from_json("Test", json_str_0, &json_result_0 , &result_len_0);

    if (result_0 == 1) {
        printf("Error code is %d. ", result_0);
        int buffer = sui_last_error_length();
        char error_msg [buffer];
        int len = sui_last_error_message_utf8(error_msg, buffer);
        printf("%s\n", error_msg);
    }

    // Transform BCS representing a Pure value into JSON
    char const *result;
    uint8_t bcs[] = { 0, 3, 49, 50, 51 };
    int r = sui_bcs_to_json("CallArg", bcs, sizeof(bcs), &result, true);
    printf("Return code is: %d\n", r);
    printf("JSON string from BCS [0,3,49,50,51] of type CallArg is:\n%s\n", result);
    sui_bcs_json_free_string(result);

    // Transform a JSON to a BCS array
    char *json_str = "{\"Pure\": [49,50,51] }";
    const uint8_t *json_result;
    size_t result_len;
    int r_json = sui_bcs_from_json("CallArg", json_str, &json_result, &result_len);
    printf("Input string is: %s. Expected BCS output is [0,3,49,50,51]\n", json_str);
    printf("Output BCS array length is: %zu, ", result_len);
    printf("BCS output is: ");
    for (size_t i=0; i<result_len; i++) {
        printf("%d,", json_result[i]);
    }
    printf("\n");
    sui_bcs_json_free_array(json_result, result_len);
    
    char *input_json = "{ \"Input\": 1000}";    
    const uint8_t *json_result1;
    size_t result_len1;
    printf("Input json string is: %s. Expected BCS output [1,232,3]\n", input_json);
    int r_2 = sui_bcs_from_json("Argument", input_json, &json_result1,&result_len1);
    printf("Output BCS array length is: %zu, ", result_len1);
    printf("BCS output is: ");
    for(int loop = 0; loop < result_len1; loop++) {
        printf("%d,", json_result1[loop]);
    }
    printf("\n");
    sui_bcs_json_free_array(json_result1, result_len1);
    
    return EXIT_SUCCESS;
}