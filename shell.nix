{
  pkgs ? import <nixpkgs> { },
}:

pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    clang
    cmake
    pkg-config
  ];

  LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
  BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${pkgs.llvmPackages.libclang.lib}/lib/clang/${pkgs.lib.getVersion pkgs.clang}/include";
  NIX_CFLAGS_COMPILE = "-Wno-error=int-conversion";
}
