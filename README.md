# ornatus

A 3D laptop customizer for the browser. Drop in a laptop `.glb` model, place PNG stickers and draw freehand on the lid, and preview it live. Written in Rust, compiled to WebAssembly, fully client-side.

Status: planning. See [ROADMAP.md](ROADMAP.md) and [docs/decisions](docs/decisions/).

The project bundles no third-party 3D models. You supply your own and are responsible for its license.

## License

Dual-licensed under MIT or Apache-2.0, at your option. You may read, fork and modify the code under those terms.

## Contributions

This is a solo project. Pull requests, issues and patches from others are not accepted.

## Development

Requires Rust (see `rust-toolchain.toml`) and [`trunk`](https://trunkrs.dev).

```bash
cargo run            # native dev window
trunk serve          # web build at http://127.0.0.1:8080
cargo xtask help     # dev tasks
```
