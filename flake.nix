{
  description = "Fast native WhatsApp client";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    # rust-toolchain.toml pins the compiler so local builds and CI agree.
    # This reads that file rather than restating the version here.
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      systems = [
        "aarch64-linux"
        "x86_64-linux"
        # Nixpkgs unstable no longer supports x86_64-darwin.
        "aarch64-darwin"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ (import rust-overlay) ];
            }
          )
        );
    in
    {
      devShells = forAllSystems (
        pkgs:
        let
          inherit (pkgs.stdenv.hostPlatform) isLinux;
        in
        {
          default = pkgs.mkShell (
            {
              packages =
                with pkgs;
                [
                  (rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
                  rust-analyzer
                  pkg-config
                  cmake
                  perl
                  llvmPackages.libclang
                ]
                # On macOS the app links the system frameworks instead.
                ++ lib.optionals isLinux [
                  alsa-lib
                  libxkbcommon
                  wayland
                  libGL
                  libx11
                  libxcursor
                  libxi
                  libxrandr
                ];
              LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
              ZAPFAST_TEST_RTL_FONT = "${pkgs.dejavu_fonts}/share/fonts/truetype/DejaVuSans.ttf";
            }
            // pkgs.lib.optionalAttrs isLinux {
              # The GUI dlopens its Wayland, X11 and GL libraries at run time.
              LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (
                with pkgs;
                [
                  libxkbcommon
                  wayland
                  libGL
                  libx11
                  libxcursor
                  libxi
                  libxrandr
                ]
              );
            }
          );
        }
      );

      packages = forAllSystems (
        pkgs:
        let
          inherit (pkgs.stdenv.hostPlatform) isLinux isDarwin;
          toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          rustPlatform = pkgs.makeRustPlatform {
            cargo = toolchain;
            rustc = toolchain;
          };
          runtimeLibs = with pkgs; [
            libxkbcommon
            wayland
            libGL
            libx11
            libxcursor
            libxi
            libxrandr
          ];
          zap-faster = rustPlatform.buildRustPackage rec {
            pname = "zap-faster";
            version = (pkgs.lib.importTOML ./Cargo.toml).package.version;
            src = self;

            # Import registry dependencies directly from Cargo.lock so ordinary
            # lock-file updates do not require refreshing a vendor hash. Git
            # dependencies remain pinned to their exact locked revisions.
            cargoLock = {
              lockFile = ./Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            nativeBuildInputs =
              with pkgs;
              [
                llvmPackages.libclang
                pkg-config
                cmake
                perl
                makeWrapper
              ]
              # Sandboxed stand-ins for iconutil and codesign in bundle.sh.
              ++ lib.optionals isDarwin [
                icnsify
                rcodesign
              ];
            # On macOS the app links the system frameworks instead.
            buildInputs = pkgs.lib.optionals isLinux (
              with pkgs;
              [
                alsa-lib
                libGL
                libx11
              ]
            );
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            ZAPFAST_TEST_RTL_FONT = "${pkgs.dejavu_fonts}/share/fonts/truetype/DejaVuSans.ttf";

            postFixup =
              # The GUI dlopens its Wayland, X11 and GL libraries at run time.
              pkgs.lib.optionalString isLinux ''
                wrapProgram $out/bin/zap-faster \
                  --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath runtimeLibs}
              ''
              # Sign after the strip hook, which would invalidate the signature.
              # The entitlement allows microphone access for voice messages.
              + pkgs.lib.optionalString isDarwin ''
                rcodesign sign \
                  --entitlements-xml-file packaging/macos/entitlements.plist \
                  "$out/Applications/Zap Faster.app"
              '';

            postInstall =
              pkgs.lib.optionalString isLinux ''
                install -Dm644 packaging/applications/zap-faster.desktop \
                  $out/share/applications/zap-faster.desktop
                install -Dm644 packaging/icons/zap-faster.svg \
                  $out/share/icons/hicolor/scalable/apps/zap-faster.svg
                install -Dm644 contrib/omarchy/zap-faster.json.tpl \
                  $out/share/zap-faster/omarchy/zap-faster.json.tpl
                install -Dm755 contrib/omarchy/zap-faster-theme \
                  $out/share/zap-faster/omarchy/zap-faster-theme
              ''
              # Mirrors packaging/macos/bundle.sh: Zap Faster.app around the built
              # binary, with the version stripped of any prerelease suffix.
              + pkgs.lib.optionalString isDarwin ''
                app="$out/Applications/Zap Faster.app"
                mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
                mv "$out/bin/zap-faster" "$app/Contents/MacOS/zap-faster"
                chmod 755 "$app/Contents/MacOS/zap-faster"
                # The command runs the bundled executable, so macOS applies the
                # bundle's Info.plist and entitlements, and start at login
                # records the bundled path.
                makeWrapper "$app/Contents/MacOS/zap-faster" "$out/bin/zap-faster"
                sed "s/__VERSION__/''${version%%-*}/g" packaging/macos/Info.plist \
                  > "$app/Contents/Info.plist"
                icnsify packaging/macos/icon-1024.png \
                  --output "$app/Contents/Resources/zap-faster.icns"
              '';

            meta = {
              description = "Fast native WhatsApp client";
              homepage = "https://github.com/eternalwaitt/zap-faster";
              license = with pkgs.lib.licenses; [ mit gpl2Only ];
              mainProgram = "zap-faster";
              # The flake publishes only aarch64-darwin. Older nixpkgs, for
              # example through follows, still list x86_64-darwin.
              platforms = pkgs.lib.platforms.linux ++ [ "aarch64-darwin" ];
            };
          };
        in
        {
          default = zap-faster;
          inherit zap-faster;
        }
      );

      formatter = forAllSystems (pkgs: pkgs.nixfmt-tree);
    };
}
