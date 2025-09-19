PodmanToolchainInfo = provider(
    fields={
        "bin": provider_field(typing.Any, default=""),
        "builder_script": provider_field(typing.Any, default=""),
    }
)


def _podman_toolchain_impl(ctx: AnalysisContext):
    return [
        DefaultInfo(),
        PodmanToolchainInfo(
            bin=ctx.attrs.bin,
            builder_script=ctx.attrs._builder_script,
        ),
    ]


system_podman_toolchain = rule(
    impl=_podman_toolchain_impl,
    attrs={
        "bin": attrs.string(),
        "_builder_script": attrs.dep(default="prelude-mysten//podman:build"),
    },
    is_toolchain_rule=True,
)
