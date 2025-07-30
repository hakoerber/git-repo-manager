{
  description = "git-repo-manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    crane = {
      url = "github:ipetkov/crane";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
  }: let
    inherit (nixpkgs) lib;

    forAllSystems = function:
      lib.genAttrs
      ["x86_64-linux" "x86_64-darwin" "aarch64-linux" "aarch64-darwin"]
      (
        system: let
          pkgs = nixpkgs.legacyPackages.${system};
        in
          function
          pkgs
          (crane.mkLib pkgs)
      );
  in {
    overlays = {
      git-repo-manager = final: prev: {
        git-repo-manager = self.packages.${prev.stdenv.system}.default;
      };
    };

    apps = forAllSystems (pkgs: _: {
      default = self.apps.${pkgs.system}.git-repo-manager;

      git-repo-manager = {
        type = "app";
        program = lib.getExe self.packages.${pkgs.system}.git-repo-manager;
      };
    });

    checks = forAllSystems (pkgs: _: {
      pkg = self.packages.${pkgs.system}.default;
      shl = self.devShells.${pkgs.system}.default;
    });

    devShells = forAllSystems (pkgs: _: {
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

    packages = forAllSystems (pkgs: craneLib: {
      default = self.packages.${pkgs.system}.git-repo-manager;

      git-repo-manager = pkgs.rustPlatform.buildRustPackage {
        name = "grm"; # otherwise `nix run` looks for git-repo-manager
        src = craneLib.cleanCargoSource (craneLib.path ./.);

        cargoLock = {
          lockFile = ./Cargo.lock;
        };

        nativeBuildInputs = [pkgs.pkg-config];
        buildInputs = with pkgs; [
          # tools
          pkg-config
          # deps
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
