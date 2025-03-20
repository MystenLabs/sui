load("@prelude//python:toolchain.bzl", "PythonToolchainInfo")
load("//toolchains/podman.bzl", "PodmanToolchainInfo")
load("//toolchains/mypkg.bzl", "MyPkgToolchainInfo")


def _get_mypkg_impl(ctx: AnalysisContext):

    meta_helper = ctx.actions.declare_output("meta.json")
    materialized_meta = ctx.actions.declare_output("materialized_meta.json")
    dst = ctx.actions.declare_output(ctx.attrs.bin)
    mypkg = ctx.attrs._mypkg_toolchain[MyPkgToolchainInfo].bin
    ctx.actions.run(
        [mypkg, cmd_args("fetch", ctx.attrs.build, "-o", dst.as_output())],
        env={"META_HELPER": meta_helper.as_output()},
        local_only=True,
        category="mypkg_fetch",
    )

    def validate_meta_json(ctx, artifacts, outputs):
        def os_enum_to_string(v):
            if v == 1:
                return "macos"
            if v == 2:
                return "linux"
            if v == 3:
                return "windows"
            fail("unknown os type: {}".format(v))

        def arch_enum_to_string(v):
            if v == 1:
                return "amd64"
            if v == 2:
                return "arm64"
            fail("unknown arch type: {}".format(v))

        meta_info = artifacts[meta_helper].read_json()
        pprint(meta_info)
        requested_arch_type = ctx.attrs.arch
        requested_os_type = ctx.attrs.os
        provided_os_type = os_enum_to_string(meta_info["os_type"])
        provided_arch_type = arch_enum_to_string(meta_info["arch_type"])
        if requested_arch_type != provided_arch_type:
            fail(
                "mismatched arch from mypkg: {}, wanted {}".format(
                    provided_arch_type, requested_arch_type
                )
            )

        if requested_os_type != provided_os_type:
            fail(
                "mismatched os from mypkg: {}, wanted {}".format(
                    provided_os_type, requested_os_type
                )
            )
        # we don't use this output, it's to satify the buck build process. annoying.
        ctx.actions.write_json(outputs[materialized_meta], meta_info)

    ctx.actions.dynamic_output(
        dynamic=[meta_helper],
        inputs=[],
        outputs=[materialized_meta],
        f=validate_meta_json,
    )

    return [
        DefaultInfo(default_outputs=[dst, materialized_meta]),
        MypkgInfo(
            build=ctx.attrs.build,
            version=ctx.attrs.version,
            arch=ctx.attrs.arch,
            os=ctx.attrs.os,
        ),
    ]


def mypkg(name: str, build: str, arch: [None, str] = None, os: [None, str] = None):
    (mypkg_name, mypkg_version) = build.split(":")
    mypkg_artifact(
        name=name,
        build=build,
        version=mypkg_version,
        arch=arch,
        os=os,
        bin=mypkg_name,
    )


mypkg_artifact = rule(
    impl=_get_mypkg_impl,
    attrs={
        "build": attrs.string(),
        "version": attrs.string(),
        "arch": attrs.string(default="amd64"),
        "os": attrs.string(default="linux"),
        "bin": attrs.string(),
        "_mypkg_toolchain": attrs.toolchain_dep(
            default="toolchains//:mypkg", providers=[MyPkgToolchainInfo]
        ),
    },
)

MypkgInfo = provider(
    fields={
        "build": provider_field(typing.Any, default=None),
        "version": provider_field(typing.Any, default=None),
        "arch": provider_field(typing.Any, default=None),
        "os": provider_field(typing.Any, default=None),
    }
)
