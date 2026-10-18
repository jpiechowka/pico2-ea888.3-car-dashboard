"""Portable production inputs shared by host and target artifact capture."""

from pathlib import Path


def shared_inputs(repo: str | Path) -> set[Path]:
    repo = Path(repo)
    paths = set()
    for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"):
        path = repo / name
        if path.is_file():
            paths.add(path)
    for crate in sorted((repo / "crates").iterdir()):
        if crate.is_dir() and (crate / "Cargo.toml").is_file():
            paths.update(p for p in crate.iterdir() if p.is_file())
            paths.update(p for p in (crate / "src").rglob("*") if p.is_file())
    paths.add(Path(__file__).resolve())
    return paths
