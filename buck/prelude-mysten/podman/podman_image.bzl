load("@prelude//python:toolchain.bzl", "PythonToolchainInfo")
load("//toolchains/podman.bzl", "PodmanToolchainInfo")


def _podman_image_impl(
    ctx: AnalysisContext,
):
    artifacts = []
    for i, dep in enumerate(ctx.attrs.layers):
        for out in dep[DefaultInfo].default_outputs:
            artifacts.append(out)

    python = ctx.attrs._python_toolchain[PythonToolchainInfo].interpreter
    builder_script = (
        ctx.attrs._podman_toolchain[PodmanToolchainInfo]
        .builder_script[DefaultInfo]
        .default_outputs
    )
    # # TODO add actual output to this image, it's just a hello world for now
    # build_script_output = ctx.actions.declare_output("{}.tar".format(ctx.attrs.name))
    # cmd = cmd_args(
    #     python,
    #     builder_script,
    #     build_script_output.as_output(),
    # )
    # ctx.actions.run(cmd, category="podman_image")
    # artifacts.append(build_script_output)
    buildah = ctx.attrs._buildah_toolchain[BuildahToolchainInfo].bin
    return [
        DefaultInfo(default_outputs=artifacts),
    ]


podman_image = rule(
    impl=_podman_image_impl,
    attrs={
        "layers": attrs.list(attrs.dep()),
        "dockerfile": attrs.string(default=""),
        "_python_toolchain": attrs.toolchain_dep(
            default="toolchains//:python", providers=[PythonToolchainInfo]
        ),
        "_podman_toolchain": attrs.toolchain_dep(
            default="toolchains//:podman", providers=[PodmanToolchainInfo]
        ),
    },
)
