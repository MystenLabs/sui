{
  inputs = {
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    #nixpkgs.url = "nixpkgs/nixos-unstable";
    # we need to wait for https://github.com/NixOS/nixpkgs/pull/387337
    # nixpkgs.url = "github:TomaSajt/nixpkgs?ref=fetch-cargo-vendor-dup";
    # rebased version on master
    nixpkgs.url = "github:poelzi/nixpkgs?ref=fetch-cargo-vendor-dup";

    genesisgit = {
      url = "github:MystenLabs/sui-genesis";
      flake = false;
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      fenix,
      flake-utils,
      nixpkgs,
      genesisgit,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        toolchain = fenix.packages.${system}.fromToolchainFile {
          file = ./rust-toolchain.toml;
          # update when toolchain changes
          sha256 = "sha256-Hn2uaQzRLidAWpfmRwSRdImifGUCAb9HeAqTYFXWeQk=";
        };
        # update this hash when dependencies changes
        cargoHash = "sha256-yH0iCfcZMQKulnz4iuImg33we66gokIoDqVDwrnpo4c=";
        # cargoHash = "";

        ##############
        node-tools = [
          "bridge-indexer"
          "deepbook-indexer"
          "stress"
          "sui-bridge-cli"
          "sui-bridge"
          "sui-cluster-test"
          "sui-faucet"
          "sui-tool"
          "sui"
        ];
        dev-tools = [
          "move-analyzer"
          "sui-light-client"
          "sui-move"
          "sui-rosetta"
          "sui"
        ];
        pkgs = nixpkgs.legacyPackages.${system};
        suiVersion = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
        llvmVersion = pkgs.llvmPackages_19;
      in
      let
        lib = pkgs.lib;
        # we use the llvm toolchain
        stdenv = pkgs.stdenvAdapters.useMoldLinker llvmVersion.libcxxStdenv;
        platform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };
        nativeBuildInputs = with pkgs; [
          git
          pkg-config
          llvmVersion.clang
          platform.bindgenHook
          llvmVersion.bintools
        ];
        basePkgs = with pkgs; [
          zstd
        ];
        genesisPkg = pkgs.stdenvNoCC.mkDerivation {
          name = "sui-genesis";
          version = suiVersion;

          src = genesisgit;

          installPhase = ''
            mkdir -p $out/share
            cp -r ${genesisgit}/devnet $out/share
            cp -r ${genesisgit}/testnet $out/share
            cp -r ${genesisgit}/mainnet $out/share
          '';
        };
      in
      let
        # builts a sui rust crate
        mkCrate =
          {
            name ? null,
            path ? null,
            bins ? null,
            genesis ? false,
            extraDeps ? [ ],
            features ? [ ],
            noDefaultFeatures ? false,
            profile ? "release",
          }:
          (platform.buildRustPackage {
            pname =
              if !builtins.isNull name then
                name
              else if !builtins.isNull path then
                path
              else
                "sui";
            version = suiVersion;
            inherit nativeBuildInputs toolchain cargoHash; # platform;

            src = lib.fileset.toSource {
              root = ./.;
              fileset = (
                lib.fileset.unions [
                  ./Cargo.toml
                  ./Cargo.lock
                  ./crates
                  ./consensus
                  ./sui-execution
                  ./external-crates
                ]
              );
            };

            cargoBuildFlags =
              (lib.lists.optionals (!builtins.isNull path) [
                "-p"
                path
              ])
              ++ (lib.optionals (!builtins.isNull bins) (
                lib.lists.concatMap (x: [
                  "--bin"
                  x
                ]) bins
              ));
            buildNoDefaultFeatures = noDefaultFeatures;
            buildFeatures = features;

            buildType = profile;

            useFetchCargoVendor = true;

            buildInputs =
              basePkgs
              ++ lib.optionals stdenv.isDarwin (
                with pkgs;
                [
                  darwin.apple_sdk.frameworks.CoreFoundation
                  darwin.apple_sdk.frameworks.CoreServices
                  darwin.apple_sdk.frameworks.IOKit
                  darwin.apple_sdk.frameworks.Security
                  darwin.apple_sdk.frameworks.SystemConfiguration
                ]
              )
              ++ extraDeps
              ++ lib.optional genesis genesisPkg;

            # preBuild = ''
            #   export GIT_REVISION="${self.rev or self.dirtyRev or "dirty"}";
            # '';

            cargoTestFlags = [
              "--profile"
              "nix"
            ];
            doCheck = false;
            useNextest = true;

            env = {
              ZSTD_SYS_USE_PKG_CONFIG = true;
            };

            outputs = [ "out" ];

            meta = with pkgs.lib; {
              description = "Sui, a next-generation smart contract platform with high throughput, low latency, and an asset-oriented programming model powered by the Move programming language";
              homepage = "https://github.com/mystenLabs/sui";
              changelog = "https://github.com/mystenLabs/sui/blob/${suiVersion}/RELEASES.md";
              license = with licenses; [
                cc-by-40
                asl20
              ];
              maintainers = with maintainers; [ poelzi ];
              mainProgram = "sui";
            };
          });
        mkDocker =
          {
            name,
            tag ? (self.rev or self.dirtyRev),
            tpkg,
            extraPackages ? [ ],
            cmd ? [ ],
            labels ? { },
            jemalloc ? false,
            debug ? false,
            genesis ? false,
          }:
          pkgs.dockerTools.buildImage {
            # pkgs.dockerTools.buildLayeredImage {

            inherit name tag;

            copyToRoot = pkgs.buildEnv {
              name = "image-${name}";
              paths =
                with pkgs.dockerTools;
                [
                  usrBinEnv
                  binSh
                  caCertificates
                  fakeNss
                ]
                ++ [
                  (
                    if debug then
                      # FIXME: why does this not set buildType do debug ?
                      (tpkg.overrideAttrs {
                        buildType = "debug";
                        separateDebugInfo = false;
                      })
                    else
                      tpkg
                  )
                ]
                ++ extraPackages
                ++ lib.optional jemalloc pkgs.jemalloc
                ++ lib.optional genesis genesisPkg
                ++ lib.optionals debug [
                  pkgs.bashInteractive
                  pkgs.coreutils
                  pkgs.gdb
                ];
              pathsToLink = [
                "/bin"
                "/lib"
                "/share"
              ];
            };

            config = {
              Cmd = cmd;
              # Cmd = ["${pkgs.bash}/bin/bash"];
              WorkingDir = "/";
              Labels = {
                "git-revision" = builtins.toString (self.rev or self.dirtyRev or "dirty");
                "build-date" = self.lastModifiedDate;
              } // labels;
              Env =
                (lib.optional jemalloc "LD_PRELOAD=${pkgs.jemalloc}/lib/libjemalloc.so")
                ++ (lib.optional debug "PS1=$(pwd) > ")
                ++ [ "PATH=/bin:" ];
            };

          };
      in
      let
        suipkgs = {
          sui-full = mkCrate { };
          sui-node = mkCrate {
            name = "sui-node";
            genesis = true;
            bins = [ "sui-node" ];
          };
          sui-dev-tools = mkCrate {
            name = "sui-dev-tools";
            bins = dev-tools;
          };
          sui-node-tools = mkCrate {
            name = "sui-tools";
            genesis = true;
            bins = node-tools;
          };
          sui-indexer = mkCrate { bins = [ "sui-indexer" ]; };
          sui-indexer-alt = mkCrate { bins = [ "sui-indexer-alt" ]; };
          # sui-light-client = mkCrate { name = "sui-light-client"; };
        };
      in
      let
        dockerImages = {
          docker-node-tools = {
            name = "sui-node-tools";
            tpkg = suipkgs.sui-node-tools;
            cmd = [ "sui" ];
            extraPackages = [ pkgs.git ];
          };
          docker-dev-tools = {
            name = "sui-dev-tools";
            tpkg = suipkgs.sui-dev-tools;
            cmd = [ "sui" ];
            extraPackages = [ pkgs.git ];
          };
          docker-node = {
            name = "sui-node";
            tpkg = suipkgs.default;
            cmd = [ "sui-node" ];
            extraPackages = [ pkgs.curl ];
            genesis = true;
            jemalloc = true;
          };
          docker-indexer = {
            name = "sui-indexer";
            tpkg = suipkgs.sui-indexer;
            cmd = [ "sui-indexer" ];
            jemalloc = true;
            extraPackages = [
              pkgs.postgresql
              pkgs.curl
            ];
          };
          docker-indexer-alt = {
            name = "sui-indexer-alt";
            tpkg = suipkgs.sui-indexer-alt;
            cmd = [ "sui-indexer-alt" ];
            jemalloc = true;
            extraPackages = [
              pkgs.postgresql
              pkgs.curl
            ];
          };

        };
      in
      {
        devShells.default = pkgs.mkShell.override { inherit stdenv; } {
          inherit nativeBuildInputs;
          RUST_SRC_PATH = "${fenix.packages.${system}.stable.rust-src}/bin/rust-lib/src";
          RUSTC_WRAPPER = "${pkgs.sccache}/bin/sccache";
          # use sccache also for c
          shellHook = ''
            export CC="sccache $CC"
            export CXX="sccache $CXX"
            # make j an alias for just
            alias j=just
            # complete -F _just -o bashdefault -o default j
          '';
          RUST_BACKTRACE = 1;

          CFLAGS = "-O2";
          CXXFLAGS = "-O2";
          buildInputs =
            nativeBuildInputs
            ++ [ toolchain ]
            ++ (with pkgs; [
              (fenix.packages."${system}".stable.withComponents [
                "clippy"
                "rustfmt"
              ])
              # turbo
              cargo-deny
              cargo-nextest
              just
              mold
              nixfmt-rfc-style
              nodePackages.webpack
              pnpm
              postgresql
              python3
              sccache
              typescript
              docker-compose
              dive
              git
              deno
            ]);
        };
        packages =
          suipkgs
          // {
            genesis = genesisPkg;
            default = suipkgs.dev-tool;
          }
          // (lib.attrsets.mapAttrs (name: spec: (mkDocker spec)) dockerImages)
          //
            # define debug versions of docker images
            (lib.attrsets.mapAttrs' (
              name: spec: (lib.nameValuePair (name + "-debug") (mkDocker (spec // { debug = true; })))
            ) dockerImages);
      }
    );
}
