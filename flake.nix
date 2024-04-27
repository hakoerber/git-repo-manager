{
  description = "git-repo-manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    crane,
    rust-overlay,
  }:
    {
      overlays = {
        git-repo-manager = final: prev: {
          git-repo-manager = self.packages.${prev.stdenv.system}.default;
        };
      };
    }
    // flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs =
          import nixpkgs
          {
            inherit system;
            overlays = [
              rust-overlay.overlays.default
            ];
          };

        rustToolchain = pkgs.rust-bin.stable.latest.default;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        environment = with pkgs; {
          pname = "grm"; # otherwise `nix run` looks for git-repo-manager
          src = craneLib.cleanCargoSource (craneLib.path ./.);
          buildInputs =
            [
              # tools
              pkg-config
              rustToolchain
              # deps
              git
              openssl
              openssl.dev
              zlib
              zlib.dev
            ]
            ++ lib.optional stdenv.isDarwin (with darwin.apple_sdk.frameworks; [
              CoreFoundation
              CoreServices
              Security
              SystemConfiguration
            ]);
        };
      in {
        apps = {
          default = self.apps.${system}.git-repo-manager;

          git-repo-manager = flake-utils.lib.mkApp {
            drv = self.packages.${system}.git-repo-manager;
          };
        };

        devShells = {
          default = pkgs.mkShell (environment
            // {
              buildInputs =
                environment.buildInputs
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
            });
        };

        packages = {
          default = self.packages.${system}.git-repo-manager;

          git-repo-manager = craneLib.buildPackage (environment
            // {
              cargoArtifacts = craneLib.buildDepsOnly environment;
            });
        };
      }
    );
}
