{
  description = "Whisper dictation app for Wayland Linux";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];

        pkgs = import nixpkgs {
          inherit system overlays;
        };

        libs = with pkgs;
          lib.makeLibraryPath [
            stdenv.cc.cc.lib
            pipewire
            alsa-lib
            gtk4
            libadwaita
            dbus
            systemd
            whisper-cpp
          ];
      in {
        devShells.default = with pkgs;
          mkShell {
            buildInputs = [
              (rust-bin.stable.latest.default.override {
                extensions = ["rust-src" "rust-analyzer"];
              })
              
              # Build tools
              pkg-config
              cmake
              clang
              llvm
              
              # Audio libraries
              pipewire
              alsa-lib
              
              # UI libraries
              gtk4
              libadwaita
              
              # System libraries
              dbus
              systemd
              
              # Whisper dependencies
              whisper-cpp
              
              # Wayland tools
              wl-clipboard
              libnotify
              
              # Development tools
              cargo-watch
              cargo-edit
            ];

            shellHook = ''
              export LD_LIBRARY_PATH=${libs}
              export PKG_CONFIG_PATH="${pipewire.dev}/lib/pkgconfig:${alsa-lib.dev}/lib/pkgconfig:${gtk4.dev}/lib/pkgconfig:${libadwaita.dev}/lib/pkgconfig:${dbus.dev}/lib/pkgconfig:$PKG_CONFIG_PATH"
              export WHISPER_CPP_LIB_DIR="${whisper-cpp}/lib"
              export WHISPER_CPP_INCLUDE_DIR="${whisper-cpp}/include"
              export LIBCLANG_PATH="${pkgs.llvmPackages_latest.libclang.lib}/lib"
              export WHISPER_DONT_GENERATE_BINDINGS=1
              echo "Dictation development environment ready!"
            '';
          };
          
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "dictation";
          version = "0.1.0";
          
          src = ./.;
          
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          
          nativeBuildInputs = with pkgs; [
            pkg-config
            cmake
            clang
            llvm
          ];
          
          buildInputs = with pkgs; [
            pipewire
            alsa-lib
            gtk4
            libadwaita
            dbus
            systemd
            whisper-cpp
            libnotify
          ];
        };
      }
    );
}
