#!/bin/bash
# Copyright (c) Mysten Labs, Inc.
# SPDX-License-Identifier: Apache-2.0

# shellcheck disable=SC2181
# This script attempts to update the Fastcrypto pointer in Sui
# It is expected to fail in some cases, notably when those updates require code changes 
set -e
set -eo pipefail

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
TOPLEVEL="${DIR}/../"
GREP=${GREP:=grep}

# Crutch for old bash versions
# Very minimal readarray implementation using read. Does NOT work with lines that contain double-quotes due to eval()
readarray() {
	while IFS= read -r var; do
        MAPFILE+=("$var")
	done
}


# check for the presence of needed executables:
# - we use GNU grep in perl re mode
function check_gnu_grep() {
	GNUSTRING=$($GREP --version|head -n1| grep 'GNU grep')
	if [[ -z $GNUSTRING ]];
	then 
		echo "Could not find GNU grep. This requires GNU grep 3.7 with PCRE expressions"; exit 1
	else 
		return 0
	fi
}

function latest_fc_revision() {
	FC_CHECKOUT=$(mktemp -d)
	cd "$FC_CHECKOUT"
	git clone --depth 1 https://github.com/mystenlabs/fastcrypto
	cd fastcrypto
	git rev-parse HEAD
}

function current_fc_revision() {
	cd "$TOPLEVEL"
	readarray -t <<< "$(find ./ -iname '*.toml' -exec $GREP -oPe 'git = "https://github.com/MystenLabs/fastcrypto", *rev *= *\"\K[0-9a-fA-F]+' '{}' \;)"
	watermark=${MAPFILE[0]}
	for i in "${MAPFILE[@]}"; do
	    if [[ "$watermark" != "$i" ]]; then
        	not_equal=true
	        break
	    fi
	done

	[[ -n "$not_equal" ]] && echo "Different values found for the current Fastcrypto revision in Sui, aborting" && exit 1
	echo "$watermark"
}

# Check for tooling
check_gnu_grep

# Debug prints
CURRENT_FC=$(current_fc_revision)
LATEST_FC=$(latest_fc_revision)
if [[ "$CURRENT_FC" != "$LATEST_FC" ]]; then
	echo "About to replace $CURRENT_FC with $LATEST_FC as the Narwhal pointer in Sui"
else
	exit 0
fi

# Edit the source
find ./ -iname "*.toml"  -execdir sed -i '' -re "s/$CURRENT_FC/$LATEST_FC/" '{}' \;
