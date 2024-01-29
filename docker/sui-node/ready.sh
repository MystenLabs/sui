#!/bin/bash

# Function to get the metric
get_metric() {
    curl -s localhost:9184/metrics | grep -v "#" | grep "last_executed_checkpoint_timestamp_ms" | cut -d " " -f 2
}

# Get the metric and current time
metric=$(get_metric)
current_time=$(date +%s%3N)

# Check if the metric is not empty
if [ -n "$metric" ]; then
    # Calculate the delta
    delta=$((current_time - metric))

    # Check if the delta is greater than 300 seconds
    if [ "$delta" -gt 300000 ]; then
        exit 1
    else
        exit 0
    fi
else
    # Handle case where metric is not available
    echo "Error: Unable to retrieve metric."
    exit 1
fi
