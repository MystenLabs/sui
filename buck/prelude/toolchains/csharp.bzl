# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This source code is licensed under both the MIT license found in the
# LICENSE-MIT file in the root directory of this source tree and the Apache
# License, Version 2.0 found in the LICENSE-APACHE file in the root directory
# of this source tree.

load("@prelude//csharp:toolchain.bzl", "CSharpToolchainInfo")

def _system_csharp_toolchain_impl(ctx):
    if not host_info().os.is_windows:
        fail("csharp toolchain only supported on windows for now")

    return [
        DefaultInfo(),
        CSharpToolchainInfo(
            csc = RunInfo(args = ctx.attrs.csc),
            framework_dirs = {
                "net35": "C:\\Program Files (x86)\\Reference Assemblies\\Microsoft\\Framework\\.NETFramework\\v3.5\\Profile\\Client",
                "net40": "C:\\Program Files (x86)\\Reference Assemblies\\Microsoft\\Framework\\.NETFramework\\v4.0",
                "net45": "C:\\Program Files (x86)\\Reference Assemblies\\Microsoft\\Framework\\.NETFramework\\v4.5",
                "net46": "C:\\Program Files (x86)\\Reference Assemblies\\Microsoft\\Framework\\.NETFramework\\v4.6",
            },
        ),
    ]

system_csharp_toolchain = rule(
    impl = _system_csharp_toolchain_impl,
    doc = """A C# toolchain that invokes the system C# compiler `csc.exe` using the current environment path.
    This toolchain requires the Microsoft provided .NET Framework SDKs (3.5, 4.0, 4.5, 4.6). By default these
    Framework SDKs should be installed at their default location, however this can be customized by changing
    the parameters passed to `system_chsarp_toolchain`.

    The `csc` and `framework_dir` attributes can be buck targets if you would like to check the C# redist bits
    into your repo.

    Usage:
  system_csharp_toolchain(
      name = "csharp",
      csc = "csc.exe",
      visibility = ["PUBLIC"],
  )""",
    attrs = {
        "csc": attrs.string(default = "csc.exe", doc = "Executable name or a path to the C# compiler frequently referred to as csc.exe"),
        "framework_dirs": attrs.dict(key = attrs.string(), value = attrs.one_of(attrs.source(), attrs.string()), doc = "Dictionary of .NET framework assembly directories, where each key is a supported value in `framework_ver` and the value is a path to a directory containing .net assemblies such as System.dll matching the given framework version"),
    },
    is_toolchain_rule = True,
)
