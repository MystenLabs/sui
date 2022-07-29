#  Copyright (c) 2022, Mysten Labs, Inc.
#  SPDX-License-Identifier: Apache-2.0

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

cd "${SCRIPT_DIR}/../" &&  cargo run -p sui -- move prove
