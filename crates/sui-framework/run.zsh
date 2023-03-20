#!/bin/zsh

export COST_TABLES="/Users/timothyzakian/cost_tables"
export COST_TABLE="$1.json"

~/work/code/sui/target/debug/sui move test -i 10000000000000 -s csv >! tiered

# SUPER HACKY
# You may need to fiddle with these nummbers if there are more or less entries in the csv output
tail -n 449 tiered | head -n 448 >! $1.csv
mv $1.csv $COST_TABLES/outputs/

unset COST_TABLES
unset COST_TABLE
