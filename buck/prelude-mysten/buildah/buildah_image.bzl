load("@prelude//python:toolchain.bzl", "PythonToolchainInfo")
load("//toolchains/buildah.bzl", "BuildahToolchainInfo")
load("//toolchains/gcloud.bzl", "GcloudToolchainInfo")
load("//mypkg:mypkg.bzl", "MypkgInfo")


def _buildah_image_impl(
    ctx: AnalysisContext,
):
    docker_root = ctx.actions.declare_output("docker_root", dir=True)
    deps = {}

    if ctx.attrs.srcs:
        for src in ctx.attrs.srcs:
            deps[src.short_path] = src

    if ctx.attrs.mapped_sources:
        deps.update(ctx.attrs.mapped_sources)

    for layer in ctx.attrs.layers:
        for dep in layer[DefaultInfo].default_outputs:
            deps[dep.short_path] = dep

    ctx.actions.copied_dir(docker_root, deps)
    buildah = ctx.attrs._buildah_toolchain[BuildahToolchainInfo].bin
    python = ctx.attrs._python_toolchain[PythonToolchainInfo].interpreter
    builder_script = (
        ctx.attrs._buildah_toolchain[BuildahToolchainInfo]
        .builder_script[DefaultInfo]
        .default_outputs
    )

    build_script_output = ctx.actions.declare_output("{}.tar".format(ctx.attrs.name))
    cmd = cmd_args(
        python,
        builder_script,
        "--buildah",
        buildah,
        "--image_name",
        ctx.attrs.name,
        "--docker_root",
        docker_root,
        "--log_level",
        ctx.attrs.buildah_log_level,
        "--out",
        build_script_output.as_output(),
    )
    if ctx.attrs.registry:
        # we only support gcloud atm
        gcloud = ctx.attrs._gcloud_toolchain[GcloudToolchainInfo].bin
        cmd.add("--gcloud", gcloud)
        cmd.add("--registry", ctx.attrs.registry)

    ctx.actions.run(cmd, category="buildah_image_and_export")
    return [
        DefaultInfo(default_outputs=[build_script_output]),
    ]


buildah_image = rule(
    impl=_buildah_image_impl,
    attrs={
        "srcs": attrs.option(attrs.list(attrs.source()), default=None),
        "layers": attrs.list(attrs.dep()),
        "registry": attrs.option(attrs.string(), default=None),
        "mapped_sources": attrs.option(
            attrs.dict(key=attrs.string(), value=attrs.source()), default=None
        ),
        "buildah_log_level": attrs.string(default="info"),
        "_python_toolchain": attrs.toolchain_dep(
            default="toolchains//:python", providers=[PythonToolchainInfo]
        ),
        "_buildah_toolchain": attrs.toolchain_dep(
            default="toolchains//:buildah", providers=[BuildahToolchainInfo]
        ),
        "_gcloud_toolchain": attrs.toolchain_dep(
            default="toolchains//:gcloud", providers=[GcloudToolchainInfo]
        ),
    },
)
