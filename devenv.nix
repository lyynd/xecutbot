{ pkgs, ... }:

{
  languages.rust = {
    enable = true;
    # https://devenv.sh/reference/options/#languagesrustchannel
    channel = "stable";

    components = [
      "rustc"
      "cargo"
      "clippy"
      "rustfmt"
      "rust-analyzer"
    ];
  };

  git-hooks.hooks = {
    rustfmt.enable = true;
    clippy.enable = true;
  };

  packages = [
    pkgs.nixfmt-rfc-style
    pkgs.openssl
    pkgs.gdb
    pkgs.nixd
  ];
}
