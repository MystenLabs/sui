@echo off
:: Copyright (c) Mysten Labs, Inc.
:: SPDX-License-Identifier: Apache-2.0
::
:: Windows sibling of the bash stub. See check_clean/stubs/prettier-move.cmd
:: for the rationale (PATHEXT requirement on Windows).

if "%~1"=="--version" (
  echo 10.0.0
  exit /b 0
)

echo ERROR: stub npm should not have been invoked with: %* 1>&2
exit /b 99
