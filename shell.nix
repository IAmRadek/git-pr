{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  name = "git-pr-dev-shell";

  buildInputs = with pkgs; [
    rustc
    cargo
    pkg-config
    openssl
  ];

  # Let pkg-config tell openssl-sys where OpenSSL is
  PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";

  # IMPORTANT: actually remove any host / global overrides,
  # not set them to an empty string.
  shellHook = ''
    unset OPENSSL_DIR
    unset OPENSSL_LIB_DIR
    unset OPENSSL_INCLUDE_DIR
  '';
}
