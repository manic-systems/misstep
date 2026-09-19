{
  mkShell,
  rustc,
  cargo,
  lld,
  rust-analyzer,
  rustfmt,
  clippy,
  taplo,
}:
mkShell {
  name = "rust";

  strictDeps = true;
  nativeBuildInputs = [
    rustc
    cargo
    lld

    (rustfmt.override {asNightly = true;})
    clippy
    cargo
    taplo

    rust-analyzer
  ];
}
