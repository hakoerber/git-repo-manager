{
  description = "git-repo-manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    crane = {
      url = "github:ipetkov/crane";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      crane,
    }:
    {
      overlays = {
        git-repo-manager = final: prev: {
          git-repo-manager = self.packages.${prev.stdenv.system}.default;
        };
      };
    }
    // flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        craneLib = crane.mkLib pkgs;
      in
      {
        apps = {
          default = self.apps.${system}.git-repo-manager;

          git-repo-manager = flake-utils.lib.mkApp {
            drv = self.packages.${system}.git-repo-manager;
          };
        };

        checks = {
          pkg = self.packages.${system}.default;
          shl = self.devShells.${system}.default;
        };

        devShells = {
          default = pkgs.mkShell {
            buildInputs = [
              self.packages.${pkgs.system}.default
            ]
            ++ (with pkgs; [
              black
              isort
              just
              mdbook
              nixfmt-classic # nix formatting
              python3
              ruff
              shellcheck
              shfmt
            ]);
          };
        };

        packages = {
          default = self.packages.${system}.git-repo-manager;

          git-repo-manager = pkgs.rustPlatform.buildRustPackage {
            name = "grm"; # otherwise `nix run` looks for git-repo-manager
            src = craneLib.cleanCargoSource (craneLib.path ./.);

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = with pkgs; [
              # deps
              git
              openssl
              openssl.dev
              zlib
              zlib.dev
            ];
            meta.mainProgram = "grm";
          };
        };
      }
    );
}
