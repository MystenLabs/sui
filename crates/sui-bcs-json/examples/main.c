// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "sui_bcs_json_bindings.h"

int main (int argc, char const * const argv[])
{
    printf("Starting BCS JSON test\n");
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

    // Transform BCS representing true bool into JSON
    uint8_t bcs_true[] = { 1 };
    char const *result;
    int r = sui_bcs_to_json("bool", bcs_true, sizeof(bcs_true), &result, true);
    printf("JSON string from BCS [1] of type bool is:\n%s\n", result);
    sui_bcs_json_free((uint8_t*)result, strlen(result));

    // Transform BCS representing number 12341 u64 into JSON
    uint8_t number_one[] = {53, 48, 0, 0, 0, 0, 0, 0};
    char const *result_1;
    int r_1 = sui_bcs_to_json("u64", number_one, sizeof(number_one), &result_1, true);
    printf("JSON string from BCS [53,48,0,0,0,0,0,0] of type u64 is:\n%s\n", result_1);
    sui_bcs_json_free((uint8_t*)result_1, strlen(result_1));

    // Transform BCS representing an address:
    uint8_t bcs_address[] = { 248, 33, 211, 72, 63, 199, 114, 94, 186, 250, 165, 163, 209, 35, 115, 212, 153, 1, 189, 252, 225, 72, 79, 33, 157, 170, 112, 102, 163, 13, 247, 125 };
    char const *result_2;
    int r_2 = sui_bcs_to_json("address", bcs_address, sizeof(bcs_address), &result_2, true);
    printf("JSON string from BCS is: %s\n", result_2);
    printf("Expected JSON string is: \"0xf821d3483fc7725ebafaa5a3d12373d49901bdfce1484f219daa7066a30df77d\"\n");


    // Transform BCS representing a Pure value into JSON
    // char const *result;
    // uint8_t bcs[] = { 0, 3, 49, 50, 51 };
    // int r = sui_bcs_to_json("pure", bcs, sizeof(bcs), &result, true);
    // printf("JSON string from BCS [0,3,49,50,51] of type  is:\n%s\n", result);
    // sui_bcs_json_free((uint8_t*)result, strlen(result));
    //
    // Transform a JSON to a BCS array
    // char *json_str = "{ 'u128': 1 }";
    // const uint8_t *json_result;
    // size_t result_len;
    // int r_json = sui_json_to_bcs("u128", json_str, &json_result, &result_len);
    // printf("Input string is: %s. Expected BCS output is [0,3,49,50,51]\n", json_str);
    // printf("BCS output: ");
    // for (size_t i=0; i<result_len; i++) {
        // printf("%d,", json_result[i]);
    // }
    // printf("\n");
    // sui_bcs_json_free(json_result, result_len);
        
    return EXIT_SUCCESS;

}
