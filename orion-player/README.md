# ANIFLEX

### Your anime library. Your machine. Your rules.

**ANIFLEX** is a locally hosted anime library built to feel like a personal streaming library, except the library lives on your own computer.

It combines a React and TypeScript interface with a Rust backend responsible for local media, playback, catalog management, progress tracking, and downloads.

ANIFLEX is designed around one main idea:

> Build your own anime library instead of depending on a permanent online streaming service.

The project can run directly on Linux or inside a container using Podman or Docker, making it possible to run the same development environment across different operating systems.

---

## What Is ANIFLEX?

ANIFLEX is a personal, offline-first anime library application. It works with media stored locally on your computer, so already-downloaded episodes remain available without an internet connection.

The application includes:

- React and TypeScript frontend
- Vite development tooling
- Rust backend and local HTTP media server
- Local video playback and subtitle support
- Local watch progress
- Anime ratings and preferences
- Experimental recommendation system
- Anime discovery and metadata
- Automated episode downloads
- Docker and Podman support
- Experimental local-network/phone access

The core library is local. Searching metadata and downloading episodes still contacts AniList and external providers.

## Features

### Local anime library and playback

Downloaded episodes and application data are stored on your computer. The player supports episode navigation, resume/continue watching, playback progress, subtitles/captions, fullscreen, and playback controls.

### Anime metadata and ratings

The catalog can store titles, English and Romaji titles, genres, format, episode count, rating, poster artwork, and synopsis. Rate anime as liked, disliked, or unrated; preferences are used by the experimental recommendation system.

### Recommendation system

The recommendation model uses library and preference history to suggest anime. Start by adding a small, varied group of anime from **Add Anime**, then rate the shows you like. Recommendations are experimental and may need more history to become useful.

### Episode downloads

The download workflow uses `ani-cli`, `yt-dlp`, and FFmpeg. When running in a container, these tools are installed inside the image, not on the host. Download availability depends on external sites and tools, which can change, rate-limit, or block requests.

For manual downloads, Orion uses AniList for anime discovery and queries HiAnime for candidate source titles. Select the matching HiAnime entry before choosing an episode. HiAnime search currently uses the site's search page; it is not a supported public API, so provider changes may require an Orion update.

## Technology

| Component | Technology |
| --- | --- |
| Frontend | React, TypeScript, Vite |
| Backend | Rust, local HTTP server |
| Media | FFmpeg |
| Downloading | ani-cli, yt-dlp |
| Containers | Docker, Podman |
| Metadata | AniList |
| Package management | npm, Cargo |

## Project Structure

The application is in `orion-player`:

```text
Aniflex/
├── orion-player/
│   ├── src/
│   │   ├── App.tsx
│   │   ├── main.tsx
│   │   ├── style.css
│   │   └── components/AnimeRequest.tsx
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── ani_cli.rs
│   │   │   ├── prediction.rs
│   │   │   ├── video_server.rs
│   │   │   └── main.rs
│   │   └── media/
│   ├── data/catalog.json
│   ├── Dockerfile
│   ├── docker-compose.yml
│   ├── package.json
│   └── vite.config.ts
├── test_data/
└── README.md
```

## Run with Podman or Docker

The same `Dockerfile` and Compose configuration work with Podman and Docker. On Linux, native Podman avoids the always-on Linux VM used by Docker Desktop. On Windows and macOS, both Podman Desktop and Docker Desktop use a Linux virtual machine.

### 1. Install a runtime

On CachyOS/Arch Linux:

```sh
sudo pacman -S podman podman-compose
```

On Fedora:

```sh
sudo dnf install podman podman-compose
```

Docker Engine and Docker Compose are also supported on Linux. On Windows or macOS, install Docker Desktop or Podman Desktop and make sure its engine/Podman machine is running.

Verify the commands for your chosen runtime:

```sh
# Podman
podman --version
podman-compose --version

# Or Docker
docker --version
docker compose version
```

Use `podman-compose` directly for Podman. The `podman compose` wrapper may select another installed Compose provider.

### 2. Get the project

```sh
git clone --branch master https://github.com/BagellXD/Aniflex.git
cd Aniflex/orion-player
```

The Compose file is inside `orion-player`, so run the commands below from that directory.

### 3. Optional local settings

No `.env` file is required. To customize settings, create it from the example:

```sh
cp .env.example .env
```

On Windows PowerShell:

```powershell
Copy-Item .env.example .env
```

For lower peak memory use during the first Rust compile, set `ORION_CARGO_BUILD_JOBS=1` in `.env`. The default is two build jobs; fewer jobs can take longer.

### 4. Build and start

Podman:

```sh
podman-compose up --build
```

Docker:

```sh
docker compose up --build
```

The first build downloads the image and dependencies and compiles Rust, so it can take several minutes. When Vite reports ready, open <http://localhost:1420>. Keep the terminal open for foreground mode and press `Ctrl+C` to stop.

To run in the background, use `podman-compose up -d` or `docker compose up -d`.

## Updating ANIFLEX

### Get the newest source changes

Run these commands from the repository root (the directory containing `orion-player`):

```sh
git status --short
git pull --ff-only origin master
```

`git status` helps you spot local edits first. If Git refuses the fast-forward because you have local changes, keep those changes and resolve or commit them before pulling; do not discard them just to update.

If the project is already running, stop it from `orion-player` first with `podman-compose down` or `docker compose down`. This stops/removes the container and network but keeps the project files, downloaded media, and named dependency caches.

### Apply updates to a Podman container

From `orion-player`, after pulling the source:

```sh
podman-compose up -d --build --force-recreate
podman-compose logs -f orion
```

The rebuild refreshes the app image and recreates the service. Keep the log command open while startup completes; it exits with `Ctrl+C` without stopping the container. Verify the service with:

```sh
podman-compose ps
```

For an ordinary code update, the source folder is bind-mounted into the container, so frontend changes may hot-reload and Rust changes are rebuilt by the development launcher. Recreate the service with `podman-compose up -d --force-recreate` if it did not pick up the changes. Rebuild the image with `--build` when the Dockerfile, installed tools, or image dependencies have changed, or when in doubt.

### Apply updates to a Docker container

From `orion-player`, after pulling the source:

```sh
docker compose up -d --build --force-recreate
docker compose logs -f orion
```

Check status with `docker compose ps`. Use `docker compose up -d --force-recreate` for ordinary source/config changes that do not change the image.

### Updating from a release archive

If you downloaded a ZIP/archive instead of cloning with Git, download and extract the new release into a separate directory, then copy your local settings and data you want to keep. Do not overwrite or delete your existing media/catalog data without a backup. Rebuild and recreate the container using the relevant command above.

### What each update command does

- `git pull --ff-only origin master` fetches and applies the latest published source when the current branch can fast-forward.
- `podman-compose up -d --force-recreate` or `docker compose up -d --force-recreate` recreates the container using the existing image and current mounted files.
- Adding `--build` rebuilds the image from the updated Dockerfile and project files before recreating the service.
- `podman-compose down` or `docker compose down` stops the service but preserves project files, media, and named caches.
- Do not use `down --volumes` for routine updates. That removes the dependency/build caches and makes the next Rust/npm startup slower. It does not remove host media, but keep backups of local data regardless.

## Container Architecture and Data

Compose runs one `orion` service:

- Vite serves the UI on port `1420`, published on the host.
- The Rust API listens on `127.0.0.1:8787` inside the container; Vite proxies API and media requests to it.
- The project directory is bind-mounted at `/app`. Downloaded media under `/app/src-tauri/media/` remains in your project directory on the host.
- `orion_node_modules` and `orion_cargo_target` are dependency/build caches, not your anime library.
- Podman and Docker use separate images and caches.

Downloaded files live in `orion-player/src-tauri/media/`. Catalog data lives in `orion-player/data/catalog.json`. Keep local media and private/local state out of Git; do not publish media unless you have the necessary rights.

## Phone and Local Network Access

Phone access is experimental and depends on runtime networking and the host firewall. The Compose default binds the UI to `127.0.0.1`, which is local-only. To try LAN access, set `ORION_BIND_ADDRESS` in `.env` to the computer's LAN/hotspot IPv4 address, recreate the service, and visit `http://<computer-ip>:1420` on the phone. Do not use `127.0.0.1` or the container's `10.x`/`172.x` address from the phone. Do not expose the development server to the public internet; keep the Rust API internal.

## Development Without Containers

The direct development path is primarily tested on Linux. Install Node.js/npm, Rust, Bash, curl, FFmpeg, and the Linux build libraries required by Tauri. From `orion-player`:

```sh
npm install
npm run dev
```

The launcher starts the Rust media server and Vite. Open <http://localhost:1420> and press `Ctrl+C` to stop. To type-check and build the frontend:

```sh
npm run build
```

Windows/macOS native development is not the primary tested workflow; use a Linux container there.

## Catalog Example

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

Media paths must match files under `src-tauri/media/`. During development, the Vite app and Rust API both need to be running for the complete experience.

## Troubleshooting

**`podman: command not found`**: Install Podman and a Compose provider from your Linux distribution. Linux does not need a Podman machine.

**`podman-compose: command not found`**: Install the `podman-compose` package. On CachyOS/Arch, use `sudo pacman -S podman-compose`; on Fedora, use `sudo dnf install podman-compose`.

**First startup takes a long time**: A cold Rust build compiles many dependencies. Wait while compilation progresses; later starts reuse the Cargo cache.

**Download cannot find `ani-cli` or FFmpeg**: The image may be old. Rebuild and recreate with `podman-compose up -d --build --force-recreate` or `docker compose up -d --build --force-recreate`, then inspect the service logs.

**Phone cannot connect**: Confirm the phone and computer share a network, `.env` uses the computer's LAN IP, the container was recreated, and the firewall allows port `1420`. Phone networking is not verified for every rootless Podman/Docker setup.

**Port `1420` is in use**: Stop the other service using the port before starting ANIFLEX.

## Privacy and Disclaimer

ANIFLEX is a personal software project and does not host or distribute a centralized anime catalog. Searching metadata or using download providers can contact external services. Availability depends on external tools/providers. Users are responsible for following applicable laws, licenses, copyright rules, and terms of service. Do not commit or distribute copyrighted media through this repository.

## AI-Assisted Development

This is a personal learning project. AI tools have been used as supporting assistance for selected tasks, including explaining Rust concepts, exploring approaches, debugging, and code review. The project direction and ongoing development belong to the author.

## Project Direction

ANIFLEX is evolving toward better local playback, more reliable downloads, stronger Rust architecture, improved media organization, better recommendations, improved cross-platform/local-network support, robust metadata handling, and a more polished interface.

## Repository

[ANIFLEX on GitHub](https://github.com/BagellXD/Aniflex)

Made by BagellXD.
