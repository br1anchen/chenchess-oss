{
  description = "Local development environment for ChenChess";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };
        devPackages = with pkgs; [
          bun
          coreutils
          # Cleanup tooling only; it never participates in a compilation.
          cargo-sweep
          jdk21_headless
          jq
          # Bun runs the workspace; Node is here only for `npm exec`, which
          # several tooling scripts shell out to for the pinned MCPJam CLI
          # (`check:mcp-e2e`, `check:mcp-apps`, `inspect:mcp`). Without it the
          # MCP end-to-end check fails as a bare ENOENT on `npm`, which reads
          # like a broken check rather than a missing toolchain.
          nodejs_22
          openssl
          pkg-config
          rustup
          stockfish
          yt-dlp
        ];
        devEnvironment = {
          RUST_BACKTRACE = "1";
          RUST_LOG = "chen_chess_coach_engine=debug,tower_http=info";
          STOCKFISH_PATH = "${pkgs.stockfish}/bin/stockfish";
          STOCKFISH_DEPTH = "16";
        };

        # mbx ships no flake and is not in nixpkgs, so the release archive is
        # pinned by digest. Linux takes the static musl build so the binary
        # needs no interpreter patching. Only the two systems this repository is
        # developed on are pinned; anywhere else `mbxSupported` is false and the
        # default shell is the uncached one.
        #
        # Only the two systems this repository is developed on are pinned.
        mbxVersion = "1.0.1";
        mbxArchives = {
          aarch64-darwin = {
            asset = "mbx-aarch64-apple-darwin.tar.gz";
            hash = "sha256-1SEN92+TZDQdPWwcUATnpzIdtFNSo00dkvmDHuPfSxw=";
          };
          x86_64-linux = {
            asset = "mbx-x86_64-unknown-linux-musl.tar.gz";
            hash = "sha256-I5LplGqtvJfSf9CFL/JC0cC0QhlQrBvTyUARFsxK+fc=";
          };
        };
        mbxSupported = builtins.hasAttr system mbxArchives;
        mbxPackage =
          let
            archive = mbxArchives.${system};
          in
          pkgs.stdenvNoCC.mkDerivation {
            pname = "mbx";
            version = mbxVersion;
            src = pkgs.fetchurl {
              url = "https://github.com/jdx/mr-boxington/releases/download/v${mbxVersion}/${archive.asset}";
              inherit (archive) hash;
            };
            # The archive holds the bare `mbx` binary with no top-level directory.
            sourceRoot = ".";
            dontConfigure = true;
            dontBuild = true;
            dontStrip = true;
            installPhase = ''
              runHook preInstall
              install -Dm755 mbx "$out/bin/mbx"
              runHook postInstall
            '';
            meta = {
              description = "Cargo build cache shared across checkouts";
              homepage = "https://mr-boxington.jdx.dev/";
              license = pkgs.lib.licenses.mit;
              mainProgram = "mbx";
            };
          };

        # No wrapper is ever exported. mbx caches per invocation (`mbx test …`),
        # and it defers to an already-set RUSTC_WRAPPER, so exporting one would
        # only guarantee nothing is cached. Release work uses `.#vanilla`, which
        # additionally keeps mbx off PATH so a stray `mbx` cannot reach a
        # release build.
        vanillaShell = pkgs.mkShell {
          packages = devPackages;
          env = devEnvironment;

          shellHook = ''
            unset RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
            echo "ChenChess vanilla dev shell"
            echo "Rust toolchain: $(rustup run 1.97.1 rustc --version 2>/dev/null || echo 'run: rustup toolchain install 1.97.1')"
            echo "Bun:  $(bun --version)"
            echo ""
            echo "Run: rustup toolchain install 1.97.1 && bun install && bun run dev"
          '';
        };
        mbxShell = pkgs.mkShell {
          packages = devPackages ++ [ mbxPackage ];
          env = devEnvironment // {
            # cargo-sweep owns target/ cleanup, so mbx must not symlink target/
            # into its own store. Without this the two double-manage the same
            # directory.
            MBX_TARGET_VIEWS = "0";
          };

          shellHook = ''
            unset RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
            echo "ChenChess dev shell (mbx)"
            echo "Rust toolchain: $(rustup run 1.97.1 rustc --version 2>/dev/null || echo 'run: rustup toolchain install 1.97.1')"
            echo "mbx: $(mbx --version)"
            echo "Policy: local store, prefix invocation, managed targets off"
            echo ""
            echo "Run: bun run test --filter=chenchess-rust"
            echo "Release work: ./tooling/nix-develop .#vanilla"
          '';
        };
      in
      {
        devShells = {
          default = if mbxSupported then mbxShell else vanillaShell;
          vanilla = vanillaShell;
        }
        // nixpkgs.lib.optionalAttrs mbxSupported {
          mbx = mbxShell;
        };
      }
    );
}
