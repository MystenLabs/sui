@echo off
:: Copyright (c) Mysten Labs, Inc.
:: SPDX-License-Identifier: Apache-2.0
::
:: Windows sibling of the bash stub. See check_clean/stubs/prettier-move.cmd
:: for the rationale (PATHEXT requirement on Windows).

if "%~1"=="--version" (
  echo stub-prettier-move 0.0.0
  exit /b 0
)

echo prettier-move called with: %*
exit /b 0
