# Author and source: https://github.com/ABelliqueux/nolibgs_hello_worlds/
{ ... }:
{
  perSystem =
    {
      pkgs,
      mipselPkgs,
      bios,
      lib,
      self',
      ...
    }:
    let
      mkpsxiso = pkgs.stdenv.mkDerivation {
        name = "mkpsxiso";

        src = pkgs.fetchFromGitHub {
          owner = "Lameguy64";
          repo = "mkpsxiso";
          rev = "633adc6778b7a7c677eecaebc7da41bd19068048";
          fetchSubmodules = true;
          hash = "sha256-HeodzX/G/0OPIjLsIGSYDUUvnxcCGg1J61CcQ/nCcwo=";
        };

        nativeBuildInputs = [
          pkgs.cmake
        ];

        cmakeFlags = [
          "-DCMAKE_BUILD_TYPE=Release"
        ];
      };

      psyq = pkgs.stdenv.mkDerivation {
        name = "psyq";

        src = pkgs.fetchurl {
          url = "http://psx.arthus.net/sdk/Psy-Q/psyq-4.7-converted-full.7z";
          hash = "sha256-1+yxw3irkeJ6myubIHEjtmHV4z6Z7HuwnoyJJP7eNmQ=";
        };

        nativeBuildInputs = [
          pkgs.p7zip
        ];

        unpackPhase = ''
          runHook preUnpack

          mkdir source
          7z x "$src" -osource

          runHook postUnpack
        '';

        postPatch = ''
          substituteInPlace source/include/libgpu.h \
            --replace-fail "extern int FntPrint();" "extern void FntPrint(...);"
        '';

        installPhase = ''
          runHook preInstall

          mkdir -p $out
          cp -r source/* $out/

          runHook postInstall
        '';

        dontFixup = true;
      };

      test-artifact =
        dir: artifacts:
        mipselPkgs.stdenv.mkDerivation {
          name = "nolibgs-test-artifact";

          src = pkgs.fetchFromGitHub {
            owner = "ABelliqueux";
            repo = "nolibgs_hello_worlds";
            rev = "48790325a9e37923dbc8c607aa4b3304316f5451";
            fetchSubmodules = true;
            hash = "sha256-eTdw/OtfNVzKJWQ4tQK9hfZJMlMe6QBq1OsJRPqHTWQ=";
          };

          nativeBuildInputs = [ mkpsxiso ];

          buildPhase = ''
            runHook preBuild

            cp -r ${psyq} psyq
            cd ${dir}/
            make all

            runHook postBuild
          '';

          installPhase = ''
            runHook preInstall

            mkdir -p $out
            cp ${lib.concatStringsSep " " artifacts} $out/

            runHook postInstall
          '';
        };

      test-suite-cd = pkgs.writeShellApplication {
        name = "nolibgs-hello-cd-test";

        runtimeInputs = [ self'.packages.frontend-desktop ];

        text = ''
          playastation-desktop \
            --bios "${bios}" \
            --bin "${
              test-artifact "hello_cd" [
                "hello_cd.bin"
                "hello_cd.cue"
              ]
            }/hello_cd.bin"
        '';
      };

      test-suite-rom =
        dir:
        pkgs.writeShellApplication {
          name = "nolibgs-hello-rom-test";

          runtimeInputs = [ self'.packages.frontend-desktop ];

          text = ''
            playastation-desktop \
              --bios "${bios}" \
              --rom "${test-artifact "${dir}" [ "${dir}.ps-exe" ]}/${dir}.ps-exe"
          '';
        };
    in
    {
      legacyPackages.nolibgs-hello-worlds = pkgs.lib.makeScope pkgs.newScope (_: {
        hello-world = test-suite-rom "hello_world";
        pad = test-suite-rom "hello_pad";
        cd = test-suite-cd;
      });
    };
}
