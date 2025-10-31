{
  description = "git-repo-manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    {
      self,
      nixpkgs,
    }:
    let
      inherit (nixpkgs) lib;
      allSystems = [
        "x86_64-linux"
        "x86_64-darwin"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = function: lib.genAttrs allSystems (sys: function sys self.legacyPackages.${sys});
    in
    {
      overlays = {
        git-repo-manager = final: prev: {
          git-repo-manager = self.packages.${prev.stdenv.system}.default;
        };
      };

      legacyPackages = forAllSystems (system: _: import nixpkgs { inherit system; });

      apps = forAllSystems (
        system: _: {
          default = self.apps.${system}.git-repo-manager;

          git-repo-manager =
            let
              inherit (self.packages.${system}) git-repo-manager;
            in
            {
              type = "app";
              inherit (git-repo-manager) meta;
              program = lib.getExe git-repo-manager;
            };
        }
      );

      checks = forAllSystems (
        system: _: {
          pkg = self.packages.${system}.default;
          shl = self.devShells.${system}.default;
        }
      );

      devShells = forAllSystems (
        system: pkgs: {
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
        }
      );

      packages = forAllSystems (
        system: pkgs: {
          default = self.packages.${system}.git-repo-manager;

          git-repo-manager = pkgs.rustPlatform.buildRustPackage {
            name = "grm"; # otherwise `nix run` looks for git-repo-manager
            src = ./.;

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
            meta = {
              description = "A git tool to manage worktrees and integrate with GitHub and GitLab";
              longDescription = ''
                GRM helps you manage git repositories in a declarative way. Configure your repositories in a TOML or YAML file, GRM does the rest.

                Also, GRM can be used to work with git worktrees in an opinionated, straightforward fashion.
              '';

              homepage = "https://hakoerber.github.io/git-repo-manager/";
              license = lib.licenses.gpl3Plus;
              mainProgram = "grm";
              maintainers = with lib.maintainers; [ siriobalmelli ];
              platforms = lib.platforms.all;
            };
          };
        }
      );
    };
}
