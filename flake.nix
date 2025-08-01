{
  description = "git-repo-manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = {
    self,
    nixpkgs,
  }: let
    inherit (nixpkgs) lib;

    forAllSystems = function:
      lib.genAttrs
      ["x86_64-linux" "x86_64-darwin" "aarch64-linux" "aarch64-darwin"]
      (system: function nixpkgs.legacyPackages.${system});
  in {
    overlays = {
      git-repo-manager = final: prev: {
        git-repo-manager = self.packages.${prev.stdenv.system}.default;
      };
    };

    apps = forAllSystems (pkgs: {
      default = self.apps.${pkgs.system}.git-repo-manager;

      git-repo-manager = {
        type = "app";
        program = lib.getExe self.packages.${pkgs.system}.git-repo-manager;
      };
    });

    checks = forAllSystems (pkgs: {
      pkg = self.packages.${pkgs.system}.default;
      shl = self.devShells.${pkgs.system}.default;
    });

    devShells = forAllSystems (pkgs: {
      default = pkgs.mkShell {
        buildInputs =
          [self.packages.${pkgs.system}.default]
          ++ (with pkgs; [
            alejandra # nix formatting
            black
            isort
            just
            mdbook
            python3
            ruff
            shellcheck
            shfmt
          ]);
      };
    });

    packages = forAllSystems (pkgs: {
      default = self.packages.${pkgs.system}.git-repo-manager;

      git-repo-manager = pkgs.rustPlatform.buildRustPackage {
        name = "grm"; # otherwise `nix run` looks for git-repo-manager

        src = lib.cleanSourceWith {
          src = lib.cleanSource ./.;

          # Core logic from https://github.com/ipetkov/crane/blob/master/lib/filterCargoSources.nix
          filter = orig: type: let
            filename = baseNameOf (toString orig);
            parentDir = baseNameOf (dirOf (toString orig));

            validFiletype = lib.any (suffix: lib.hasSuffix suffix filename) [
              # Keep rust sources
              ".rs"
              # Keep all toml files as they are commonly used to configure other
              # cargo-based tools
              ".toml"
            ];
          in
            validFiletype
            # Cargo.toml already captured above
            || filename == "Cargo.lock"
            # .cargo/config.toml already captured above
            || parentDir == ".cargo" && filename == "config"
            || type == "directory";
        };

        cargoLock = {
          lockFile = ./Cargo.lock;
        };

        nativeBuildInputs = [pkgs.pkg-config];
        buildInputs = with pkgs; [
          git
          openssl
          openssl.dev
          zlib
          zlib.dev
        ];

        meta.mainProgram = "grm";
      };
    });
  };
}
