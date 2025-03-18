class SuiProver < Formula
  desc "Sui Prover - a tool for verifying Move smart contracts on the Sui blockchain"
  homepage "https://github.com/asymptotic-code/sui" 
  url "https://github.com/asymptotic-code/sui" branch: "${{ github.ref_name }}"
  version "1.0.0"
  license "Apache-2.0"

  depends_on "dotnet@8"
  depends_on "rust" => :build
  depends_on "z3"

  def install
    # Assume the repository is already cloned into `buildpath`
    # This means you should fetch it in your GitHub Action and pass the path to Homebrew

    # Install Rust package from the local source
    system "cargo", "install", "--locked", "--path", "./crates/sui-move", "--features", "all"

    libexec.install "#{buildpath}/target/release/sui-move"

    # Setup .NET environment
    ENV.prepend_path "PATH", Formula["dotnet@8"].opt_bin
    ENV["DOTNET_ROOT"] = Formula["dotnet@8"].opt_libexec

    # Use locally staged Boogie instead of fetching from Git
    (buildpath/"boogie").install Dir["#{buildpath}/boogie-source/*"]

    (buildpath/"boogie").cd do
      system "dotnet", "build", "Source/Boogie.sln", "-c", "Release"
      libexec.install Dir["Source/BoogieDriver/bin/Release/net8.0/*"]
      bin.install_symlink libexec/"BoogieDriver" => "boogie"
    end

    # Create an environment wrapper for `sui-move`
    (bin/"sui-move").write_env_script libexec/"sui-move", {
      DOTNET_ROOT: Formula["dotnet@8"].opt_libexec,
      BOOGIE_EXE:  bin/"boogie",
      Z3_EXE:      Formula["z3"].opt_bin/"z3",
    }
  end

  def caveats
    <<~EOS
      The formal verification toolchain has been installed.
    EOS
  end

  test do
    system "z3", "--version"
    system "#{bin}/sui-move", "--version"
  end
end
