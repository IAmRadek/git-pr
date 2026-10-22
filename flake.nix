{
  description = "git-pr - opinionated PR creation with related PR tracking";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs = { self, nixpkgs }:
    let
      lib = nixpkgs.lib;
      systems = [ "aarch64-darwin" "aarch64-linux" "x86_64-linux" ];
      forAllSystems = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      version = (lib.importTOML ./Cargo.toml).package.version;

      # Builds and tests the crate. git2 links libgit2, which needs openssl,
      # libssh2 and zlib.
      mkPackage = pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = "git-pr";
          inherit version;
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.openssl pkgs.libssh2 pkgs.zlib ];
          # get_config_dir creates ~/.config/git-pr, and the sandbox HOME is read-only
          preCheck = ''
            export HOME=$(mktemp -d)
          '';
          meta.mainProgram = "git-pr";
        };
    in {
      packages = forAllSystems (pkgs:
        let git-pr = mkPackage pkgs;
        in {
          inherit git-pr;
          default = git-pr;
        });

      checks = forAllSystems (pkgs: self.packages.${pkgs.stdenv.hostPlatform.system});

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.git-pr ];
          packages = [ pkgs.clippy pkgs.rustfmt pkgs.rust-analyzer pkgs.gh pkgs.glab ];
        };
      });

      overlays.default = final: prev: {
        git-pr = mkPackage final;
      };
    };
}
