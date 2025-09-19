BuildahToolchainInfo = provider(
    fields={
        "bin": provider_field(typing.Any, default=""),
        "builder_script": provider_field(typing.Any, default=""),
        "export_script": provider_field(typing.Any, default=""),
    }
)


def _buildah_toolchain_impl(ctx: AnalysisContext):
    return [
        DefaultInfo(),
        BuildahToolchainInfo(
            bin=ctx.attrs.bin,
            builder_script=ctx.attrs._builder_script,
            export_script=ctx.attrs._export_script,
        ),
    ]


system_buildah_toolchain = rule(
    impl=_buildah_toolchain_impl,
    attrs={
        "bin": attrs.string(),
        "_builder_script": attrs.dep(default="prelude-mysten//buildah:build"),
        "_export_script": attrs.dep(default="prelude-mysten//buildah:export"),
    },
    is_toolchain_rule=True,
)
