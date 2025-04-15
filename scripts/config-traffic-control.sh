#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

usage() {
    echo "Usage: $0 <search_string>"
    echo
    echo "Provides a recommended configuration for the client-id-source "
    echo "field of the sui-node policy-config config for traffic controller."
    echo "To use, do the following:"
    echo
    echo "1. Set the following sui-node config:"
    echo
    echo "  client-id-source:"
    echo "    x-forwarded-for: 0"
    echo
    echo "2. Start the node"
    echo "3. Run this script, piping sui-node logs to it and providing the known client IP address as an argument."
    echo "4. The script will output the recommended configuration for the client-id-source field."
    echo "5. Set the client-id-source field to the recommended configuration."
    echo "6. Restart the node."
    echo "7. The node will now use the recommended configuration for the client-id-source field."
    echo
    echo "NOTE: If the node is not running behind a proxy, this script will not yield any results."
    echo "      In such a case, set the client-id-source field to the default value of 'socket-addr'."
    echo
    echo "Example 1: journalctl -fu sui-node | $0 1.2.3.4"
    echo "Example 2: echo 'x-forwarded-for contents: [\"1.2.3.4\", \"5.6.7.8\", \"4.5.6.7\"]' | $0 1.2.3.4"
}

# Check for help flag
if [ "$1" = "-h" ] || [ "$1" = "--help" ]; then
    usage
    exit 0
fi

# Check that a search string is provided
if [ $# -ne 1 ]; then
    usage
    exit 1
fi

search="$1"

while IFS= read -r line; do
    # Check if the line matches the pattern for x-forwarded-for
    # Using a regex with a capturing group to extract the contents inside the brackets.
    if [[ $line =~ x-forwarded-for[[:space:]]+contents:[[:space:]]+\[(.*)\]\. ]]; then
        inside_brackets="${BASH_REMATCH[1]}"

        # Replace '", "' with newlines to split into multiple lines, then read into an array
        IFS=$'\n' read -d '' -r -a items < <(echo "$inside_brackets" | sed 's/", "/\n/g')

        # Strip any non-integer characters from start and end of each item
        for i in "${!items[@]}"; do
            items[$i]=$(echo "${items[$i]}" | sed 's/^[^0-9]*//; s/[^0-9]*$//')
        done

        # Store the entire array into a variable (space-separated)
        contents_var="${items[@]}"

        # Find the index of the search element
        found_index=-1
        for i in "${!items[@]}"; do
            if [ "${items[$i]}" = "$search" ]; then
                found_index=$i
                break
            fi
        done

        if [ $found_index -ge 0 ]; then
            # Calculate how many elements come after the found element
            elements_after=$(( ${#items[@]} - (found_index + 1) ))
            result=$(( 1 + elements_after ))

            # Print the contents array and the recommended configuration
            echo "x-forwarded-for contents: $contents_var"
            echo "Configuration:"
            echo "  client-id-source:"
            echo "    x-forwarded-for: $result"

            exit 0
        fi
    fi
done

# If we get here, no match was found
exit 1
