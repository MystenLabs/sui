@echo off
:: Copyright (c) Mysten Labs, Inc.
:: SPDX-License-Identifier: Apache-2.0
::
:: Windows sibling of the bash stub. `which::which` on Windows requires a
:: PATHEXT-matched extension, so we ship both files; Unix `which` only sees
:: the no-extension one, Windows `which` only sees this one.

if "%~1"=="--version" (
  echo stub-prettier-move 0.0.0
  exit /b 0
)

echo prettier-move called with: %*
exit /b 0
