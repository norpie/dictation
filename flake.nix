{
  description = "Whisper dictation app for Wayland Linux";

  inputs = {
    nixpkgs.url = "path:/home/norpie/repos/nixpkgs";
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
            rocmPackages.clr
            rocmPackages.rocblas
            rocmPackages.hipblas
            rocmPackages.hiprand
            rocmPackages.rocrand
            rocmPackages.rocprim
            rocmPackages.rocthrust
            rocmPackages.hipcub
            rocmPackages.rocfft
            rocmPackages.miopen
            zstd
            zlib
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
              just
              cmake
              clang
              llvm
              lld

              # Python for daemon
              python313
              python313Packages.pip
              python313Packages.virtualenv
              python313Packages.ctranslate2-rocm
              # (python313Packages.torch.override { rocmSupport = true; })
              # (python313Packages.torchaudio.override { rocmSupport = true; })

              # Audio libraries
              pipewire
              alsa-lib
              portaudio

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

              # ROCm configuration for AMD RX 7900 XTX
              export HSA_OVERRIDE_GFX_VERSION=11.0.0
              export AMDGPU_TARGETS=gfx1100
              export HIP_VISIBLE_DEVICES=0
              export CUDA_VISIBLE_DEVICES=0
              export PYTORCH_ROCM_ARCH=gfx1100
              export HSA_ENABLE_SDMA=0

              # Python virtual environment with system site packages
              if [ ! -d "daemon-py/.venv" ]; then
                python3 -m venv daemon-py/.venv --system-site-packages
              fi
              source daemon-py/.venv/bin/activate

              echo "Dictation development environment ready with ROCm GPU support and Python venv!"
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
