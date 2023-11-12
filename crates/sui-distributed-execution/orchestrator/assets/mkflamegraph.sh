#!/bin/bash -ex

sudo perf record -F 99 -g -t $1 -- sleep 30
sudo perf script | ./stackcollapse-perf.pl > out.perf-folded
./flamegraph.pl out.perf-folded > perf.svg
