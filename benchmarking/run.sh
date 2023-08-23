#!/bin/bash

# Parameters check
if [[ $1 == "" ]]
then
  echo "Please provide the name of the benchmark to run."
  echo "Usage: $0 [benchmark_name]"
  exit
fi

# Ugly hack: Pre-execute to parse the highest checkpoint from the program output.
./simple_channel_executor --execute "1" --download "1" --config-path "./data/config/$1.yaml" > ./data/logs/pre_exec.out

highest_checkpoint=$(cat ./data/logs/pre_exec.out | head -n 1 | cut -d " " -f 3)

# Execute benchmark up to the determined checkpoint watermark.
./simple_channel_executor --execute "$highest_checkpoint" --download "$highest_checkpoint" --config-path "./data/config/$1.yaml"
