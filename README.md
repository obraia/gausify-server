# Gausify Server

A single-binary local server for a [Gausify](https://gausify.app) splat library.
Drop it in the folder where your conversions are saved, run it, and paste the
printed HTTPS URL into the Gausify gallery — no nginx, no config.

It exists so you can **host and share the gaussians you generate on Gausify
however you like**, from your own machine, without uploading them anywhere. The
platform links to this project so any user can download the server, run it over
their own library, and open it in the gallery.

Under the hood it is a **drop-in replacement for an nginx static host**: it
serves the library's static files (`manifest.json`, `thumb.webp`,
`.sog`/`.ply`/`.splat` frames) and returns JSON directory listings in the exact
shape the gallery crawls, with CORS and HTTP Range support built in. If you'd
rather run nginx, an equivalent config is provided — see
[Running with nginx instead](#running-with-nginx-instead).

- **Language:** Rust (single static binary, no runtime dependencies)
- **Platforms:** Windows, macOS, Linux
- **License:** MIT

## Download

Grab a prebuilt binary from the [Releases](../../releases) page:

| Platform | Asset |
| --- | --- |
| Windows (x64) | `gausify-server-x86_64-pc-windows-msvc.exe` |
| macOS (Apple Silicon) | `gausify-server-aarch64-apple-darwin` |
| Linux (x64) | `gausify-server-x86_64-unknown-linux-gnu` |

On macOS/Linux, mark it executable: `chmod +x gausify-server-*`.

Prefer to build it yourself? See [Build from source](#build-from-source).

## Usage

```bash
# From inside your library folder:
gausify-server

# …or point it anywhere:
gausify-server --library /path/to/library
```

Output:

```
  Gausify Server
  Serving library:
    /path/to/library

  HTTPS:
    https://localhost:8443
    https://192.168.1.10:8443
  HTTP:
    http://localhost:8080
    http://192.168.1.10:8080

  Paste an HTTPS URL above into the Gausify gallery. Ctrl-C to stop.
```

Open the Gausify gallery, choose **Load from URL**, and paste
`https://192.168.1.10:8443/`.

### Expected library layout

The same layout the app's "Save to folder" export produces:

```
library/
├── 3dgs/
│   └── my-object/
│       ├── manifest.json
│       ├── thumb.webp
│       └── frame.sog
└── 4dgs/
    └── my-video/
        ├── manifest.json
        ├── thumb.webp
        └── frames/
            └── frame0001.sog
```

New folders appear automatically — the listing is read from disk on each
request, so there is nothing to re-index after adding assets.

## HTTPS and the self-signed certificate

On first run the server generates a self-signed certificate covering
`localhost`, `127.0.0.1` and every detected LAN IP, cached under
`<library>/.gausify/`. Because it is self-signed, the browser will not trust it
automatically:

- **Open `https://<ip>:8443/` once in the browser and accept the certificate.**
  Otherwise the app's `fetch()` fails with a generic network error.

An HTTPS web page can only fetch HTTPS resources (mixed-content rule), so if the
Gausify app is served over HTTPS, use the HTTPS URL. The plain-HTTP listener is
there for same-scheme / local testing.

## Configuration

Flags:

| Flag | Default | Description |
| --- | --- | --- |
| `--library <path>` | current dir | Folder to serve |
| `--http-port <port>` | `8080` | Plain-HTTP port |
| `--https-port <port>` | `8443` | HTTPS port |
| `--no-http` | off | Disable the HTTP listener |
| `--no-https` | off | Disable the HTTPS listener |
| `--config <path>` | `./gausify.toml` | Config file location |

Anything a flag sets can also live in `gausify.toml`; flags win over the file.
See [`config.toml`](config.toml) for an annotated example — copy it to
`gausify.toml` next to the binary and uncomment what you need.

Precedence: **command-line flag > `gausify.toml` value > built-in default.**

## HTTP surface

Everything below the root is served from the library folder:

- **`GET /<dir>/`** → JSON array of entries, one per non-dotfile child:
  ```json
  [
    { "name": "my-object", "type": "directory", "mtime": "…", "size": 0 },
    { "name": "manifest.json", "type": "file", "mtime": "…", "size": 812 }
  ]
  ```
  This matches nginx's `autoindex_format json`. Entries are sorted by name and
  dotfiles are hidden.
- **`GET /<path/to/file>`** → the file, with `Content-Type`, `Content-Length`,
  `ETag`, `Last-Modified`, and full **Range** / partial-content support.
  `.sog` / `.ply` / `.splat` frames are sent with
  `Cache-Control: public, max-age=31536000, immutable`.
- CORS is open (`Access-Control-Allow-Origin: *`) for `GET`/`HEAD`/`OPTIONS`.

Extra endpoints:

- **`GET /health`** → `{"status":"ok"}`
- **`GET /stats`** → `{"requests":…,"bytesServed":…,"uptimeSeconds":…,"library":"…"}`

## Build from source

Requires a [Rust toolchain](https://rustup.rs) (stable).

Native build (for the machine you're on):

```bash
cargo build --release
# binary at target/release/gausify-server(.exe)
```

Crypto is [`ring`](https://crates.io/crates/ring) (not aws-lc-rs), so there is
**no cmake/NASM requirement** and the binary cross-compiles cleanly.

### A Windows .exe from GitHub Actions (recommended)

No local Windows toolchain needed — a Windows runner builds a native MSVC `.exe`.

1. Open the **Actions** tab → **release** → **Run workflow**.
2. When it finishes, download the **gausify-server-x86_64-pc-windows-msvc**
   artifact — it contains `gausify-server-x86_64-pc-windows-msvc.exe`.

Pushing a tag such as `v0.1.0` does the same and also publishes a GitHub Release
with the Windows, macOS and Linux binaries attached.

### A Windows .exe locally (cross-compile from macOS/Linux)

```bash
# one-time toolchain (macOS; use your distro's package manager on Linux)
brew install mingw-w64
rustup target add x86_64-pc-windows-gnu

# build
cargo build --release --target x86_64-pc-windows-gnu
# -> target/x86_64-pc-windows-gnu/release/gausify-server.exe
```

Copy the `.exe` to the Windows machine, drop it in the library folder and run it
(Windows may prompt about the firewall the first time — allow it on your private
network). The GitHub Actions build above produces an MSVC binary instead of this
GNU one; both work.

## Running with nginx instead

If you already run nginx, [`deploy/nginx/gausify.conf`](deploy/nginx/gausify.conf)
reproduces the binary's behavior exactly: JSON autoindex, open CORS, Range
support, immutable caching for splat frames, and hidden dotfiles.

```bash
# 1. Point `root` in the config at your library folder, and set the SSL cert
#    paths (the file has an openssl one-liner to mint a self-signed cert).
# 2. Install it:
sudo cp deploy/nginx/gausify.conf /etc/nginx/sites-available/gausify.conf
sudo ln -s /etc/nginx/sites-available/gausify.conf /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

Then paste `https://<host>:8443/` into the gallery, the same as with the binary.
The one behavioral note: nginx returns the JSON listing only for URLs ending in
`/` (it 301-redirects a bare directory path to the trailing-slash form first),
which is what the gallery crawler requests anyway.

## How it works

| File | Responsibility |
| --- | --- |
| [`src/main.rs`](src/main.rs) | Wires up the HTTP + HTTPS listeners and the startup banner |
| [`src/config.rs`](src/config.rs) | CLI flags + `gausify.toml`, merged into effective settings |
| [`src/serve.rs`](src/serve.rs) | The HTTP surface: JSON autoindex, file serving, CORS |
| [`src/tls.rs`](src/tls.rs) | Self-signed cert generation / caching under `.gausify/` |
| [`src/net.rs`](src/net.rs) | LAN IPv4 discovery for the banner and cert SANs |

## Contributing

Issues and pull requests are welcome. Please run `cargo fmt` and
`cargo clippy` before opening a PR.

## License

[MIT](LICENSE) © obraia
