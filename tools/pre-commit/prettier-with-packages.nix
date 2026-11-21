{
  lib,
  stdenv,
  pnpm_11,
  nodejs_22,
  cacert,
}: let
  # This derivation needs to be manually updated every time the pnpm-lock.yaml file changes.
  # The output differs by platform: pnpm installs the darwin-only fsevents package on macOS.
  outputHash =
    if stdenv.hostPlatform.isDarwin
    then "sha256-vUs6vitV+pS25xK6s4Bt73IYu+irhR/LooseaAQOld8="
    else "sha256-v8WYmVIFfilhFscp6cqPJH2e+BvQhjrN/dgS0HD1rsE=";
in
  stdenv.mkDerivation {
    pname = "prettier-node-modules";
    version = "0.0.0";

    src = ./.;

    outputHashAlgo = "sha256";
    outputHashMode = "recursive";
    inherit outputHash;

    nativeBuildInputs = [
      pnpm_11
      cacert
      nodejs_22
    ];

    buildPhase = ''
      set -e
      # Default home at this point is /homeless-shelter, which doesn't exist and breaks pnpm
      echo "HOME was ''${HOME}"
      export HOME=$(mktemp -d fake-homeXXXX --tmpdir)
      echo "HOME is now ''${HOME}"

      export NODE_EXTRA_CA_CERTS="${cacert}/etc/ssl/certs/ca-bundle.crt"
      pnpm install --frozen-lockfile --ignore-scripts

      # they have timestamps, which breaks the hashes. See also https://github.com/pnpm/pnpm/issues/11669
      rm node_modules/.modules.yaml node_modules/.pnpm-workspace-state-v1.json
    '';

    installPhase = ''
      mkdir -p $out
      ls -l
      cp package.json pnpm-lock.yaml .prettierrc.json $out/
      cp -r node_modules $out/
      ${lib.optionalString stdenv.hostPlatform.isDarwin ''
        # macOS builds are not sandboxed, so pnpm's bin shims embed the per-build
        # directory and the hash changes on every build. Rewrite it to the fixed path
        # that Linux sandbox builds already embed. A fixed-output derivation cannot
        # reference store paths, so this must not be $out.
        grep -rlFI "$PWD" $out/node_modules | xargs -r sed -i "s|$PWD|/build/pre-commit|g"
      ''}
    '';

    # This is a fixed-output derivation. Patching nix store paths into it would
    # break it, so disable all automatic adjustments.
    dontFixup = true;
    dontPatch = true;
    dontPatchShebangs = true;
  }
