{
  description = "git-repo-manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    crane = {
      url = "github:ipetkov/crane";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    rust-overlay,
  }: let
    inherit (nixpkgs) lib;

    mkCraneLib = pkgs:
      (crane.mkLib pkgs).overrideToolchain
      pkgs.rust-bin.stable.latest.default;

    mkEnvironment = pkgs: let
      rustToolchain = pkgs.rust-bin.stable.latest.default;
      craneLib = mkCraneLib pkgs;
    in {
      pname = "grm"; # otherwise `nix run` looks for git-repo-manager

      src = craneLib.cleanCargoSource (craneLib.path ./.);
      buildInputs = with pkgs;
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

      meta.mainProgram = "grm";
    };

    forAllSystems = function:
      lib.genAttrs
      ["x86_64-linux" "x86_64-darwin" "aarch64-linux" "aarch64-darwin"]
      (
        system: let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [
              rust-overlay.overlays.default
            ];
          };
        in
          function
          pkgs
          (mkEnvironment pkgs)
          (mkCraneLib pkgs)
      );
  in {
    overlays = {
      git-repo-manager = final: prev: {
        git-repo-manager = self.packages.${prev.stdenv.system}.default;
      };
    };

    apps = forAllSystems (pkgs: _: _: {
      default = self.apps.${pkgs.system}.git-repo-manager;

      git-repo-manager = {
        type = "app";
        program = lib.getExe self.packages.${pkgs.system}.git-repo-manager;
      };
    });

    checks = forAllSystems (pkgs: _: _: {
      pkg = self.packages.${pkgs.system}.default;
      shl = self.devShells.${pkgs.system}.default;
    });

    devShells = forAllSystems (pkgs: environment: _: {
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
    });

    packages = forAllSystems (pkgs: environment: craneLib: {
      default = self.packages.${pkgs.system}.git-repo-manager;

      git-repo-manager = craneLib.buildPackage (environment
        // {
          cargoArtifacts = craneLib.buildDepsOnly environment;
        });
    });
  };
}
