# Orion Player

Orion Player is a locally hosted anime library website. The interface is built with React, TypeScript, and Vite; a Rust HTTP server provides the local catalog, playback, and API routes.

## Requirements

- CachyOS, Arch Linux, or another Linux distribution with Bash and `curl`
- Node.js 20 or newer and npm
- Rust stable (install with rustup)
- FFmpeg, including `ffprobe`, for video inspection and conversion features
- `ani-cli` for anime search/download features

On CachyOS or Arch Linux, install the development packages with:

```sh
sudo pacman -S --needed base-devel curl wget file openssl libxdo libayatana-appindicator librsvg webkit2gtk-4.1 nodejs npm rustup ffmpeg
rustup default stable
```

The Rust package still includes Tauri build dependencies, so Linux system libraries used by Tauri may be needed when Cargo compiles the backend. The website itself runs in a regular browser and does not need the Tauri desktop shell.

## Run Locally

From the `orion-player` directory:

```sh
npm install
npm run dev
```

The dev command starts the Rust HTTP server and Vite. Open the local URL printed by Vite (usually `http://localhost:1420`). The first run downloads and compiles the Rust and npm dependencies. Keep the terminal open while using the site; press `Ctrl+C` to stop both services.

To create a production frontend build:

```sh
npm run build
```

The build output is written to `dist/`. API and media routes still need the Rust server when running the full site; a static frontend host alone will not provide those routes.

## Operating Systems

### Linux

Linux is the current supported development environment. The documented setup and `npm run dev` launcher are intended to run directly on Linux.

### Windows workaround

Native Windows development is not currently supported or tested. One possible workaround is to run the Rust backend in a Linux-based container and use the website from a browser on Windows. This repository does not yet include a Dockerfile or Compose configuration, so the container setup still needs to be created; changing the bind address alone is not enough.

Inside the Linux container, edit `orion-player/src-tauri/src/video_server.rs` and change the server bind address from loopback:

```rust
Server::http(("127.0.0.1", PORT))
```

to all container interfaces:

```rust
Server::http(("0.0.0.0", PORT))
```

Do this for the containerized backend, not for a normal local run. `0.0.0.0` makes the server listen on every network interface available to it. Only expose the needed ports, and do not expose the service to the public internet. For phone testing, connecting the laptop and phone through the phone's hotspot may reduce which devices can reach the laptop, but a hotspot is not a security guarantee; anyone or anything able to join or reach that network may be able to contact exposed services.

The website frontend itself is browser-based, but that does not make the Rust API/backend cross-platform. More portable networking and a supported container setup are planned for a future version.

## Optional Media Tools

FFmpeg/`ffprobe` and `ani-cli` are external programs, not npm or Cargo packages. Install them separately and make them available on `PATH` to use the related search, download, and conversion features. Basic frontend development does not require anime downloads, but the Rust API server is needed for catalog, playback, and API functionality.

## Catalog

Edit `data/catalog.json` as an array of local entries. Example:

```json
[
  {
    "id": "sample-show",
    "title": "Sample Show",
    "year": 2024,
    "format": "TV",
    "genres": ["Adventure", "Fantasy"],
    "synopsis": "A short local-library description.",
    "poster": "/media/sample-show-poster.jpg",
    "video": "/media/sample-show.mp4"
  }
]
```

Poster and video paths refer to local media. Downloaded video files and generated local state are ignored by Git; add your own media locally under `src-tauri/media/` and do not commit copyrighted downloads.

## AI Assistance

I built this project myself and used AI as a supporting tool for selected parts of the work. Since I am still learning Rust, I especially used it to help explain Rust concepts, explore approaches, and work through some code. The project and its direction are mine, and I continue to review, test, and learn from the code.
