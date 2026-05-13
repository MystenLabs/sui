@REM Copyright (c) Mysten Labs, Inc.
@REM SPDX-License-Identifier: Apache-2.0
@echo off

if "%~1"=="--version" (
    echo 10.0.0
    exit /b 0
)

>&2 echo ERROR: stub npm should not have been invoked with: %*
exit /b 99
