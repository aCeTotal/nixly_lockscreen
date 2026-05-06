{
  description = "nixly_lockscreen - Wayland session locker in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        nativeBuildInputs = with pkgs; [
          rustc
          cargo
          rustfmt
          clippy
          rust-analyzer
          pkg-config
        ];

        buildInputs = with pkgs; [
          wayland
          wayland-protocols
          libxkbcommon
          vulkan-loader
          libGL
          pam
          udev
          fontconfig
          freetype
        ];

        runtimeLibs = pkgs.lib.makeLibraryPath buildInputs;

        cargoLockExists = builtins.pathExists ./Cargo.lock;

        package = pkgs.rustPlatform.buildRustPackage {
          pname = "nixly-lockscreen";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          inherit nativeBuildInputs buildInputs;
          postFixup = ''
            for bin in $out/bin/*; do
              patchelf --add-rpath ${runtimeLibs} "$bin" || true
            done
          '';
          meta.mainProgram = "nixly-lockscreen";
        };
      in
      {
        devShells.default = pkgs.mkShell {
          inherit nativeBuildInputs buildInputs;
          LD_LIBRARY_PATH = runtimeLibs;
          shellHook = ''
            echo "nixly_lockscreen dev shell"
            echo "  cargo build              -> compile workspace"
            echo "  cargo run -p locker      -> run lockscreen"
            echo "  cargo run -p idle        -> run idle daemon"
          '';
        };
      } // (if cargoLockExists then {
        packages.default = package;
        packages.nixly-lockscreen = package;

        apps.default = {
          type = "app";
          program = "${package}/bin/nixly-lockscreen";
        };
        apps.idled = {
          type = "app";
          program = "${package}/bin/nixly-idled";
        };
        apps.lockguard = {
          type = "app";
          program = "${package}/bin/nixly-lockguard";
        };
      } else { })
    ) // {
      nixosModules.default = import ./module.nix { inherit self; };
      nixosModules.nixly-lockscreen = import ./module.nix { inherit self; };
    };
}
