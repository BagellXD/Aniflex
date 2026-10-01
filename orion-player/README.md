# Orion Player

**Version 1.1.0 - Forced Patch**

Orion Player is a locally hosted anime library website. The browser UI uses React, TypeScript, and Vite. A Rust HTTP server handles the local catalog, playback, progress, and API requests. The Docker workflow runs both processes together in a Linux container.

This is a personal project and a development setup, not a public production service. Streaming/download availability depends on external sites and tools and can change at any time.

## What Changed in 1.1

- Added a Docker-based Linux development environment.
- Added `ani-cli`, `yt-dlp`, FFmpeg, and `fzf` to the container for the app's download flow.
- Kept the Rust API on container loopback; only the Vite website port is published.
- Added a longer startup wait for the first Rust compile.
- Added phone-over-hotspot instructions. Phone access through Docker Desktop for Linux is **not yet verified**; see the phone section below.

## Quick Start: Docker

Docker is the recommended way to run this version. It supplies Node, Rust, Linux build libraries, and media command-line tools inside the container, so you do not need to install those development dependencies on the host.

### 1. Install and start Docker

Install Docker Desktop for your operating system, start it, and wait for its engine to report that it is running. On Linux, Docker Engine with the Compose plugin also works. Avoid installing overlapping Docker Desktop and system Compose packages if your distribution reports a file conflict.

Check that the CLI can reach the engine:

```sh
docker --version
docker compose version
docker context ls
```

If Docker Desktop is being used, select its context if necessary:

```sh
docker context use desktop-linux
```

### 2. Get the project

Clone the repository, then enter the app directory:

```sh
git clone https://github.com/BagellXD/Aniflex.git
cd Aniflex/orion-player
```

If you already have the repository, open a terminal in its `orion-player` directory instead.

### 3. Create local settings

On Linux/macOS/Git Bash:

```sh
cp .env.example .env
```

On Windows PowerShell:

```powershell
Copy-Item .env.example .env
```

The default `ORION_BIND_ADDRESS=127.0.0.1` makes the website available only from the laptop itself, which is the safer default.

### 4. Build and start Orion

From `orion-player`:

```sh
docker compose up --build
```

The first build downloads a Linux image, installs system packages, Rust and npm dependencies, then compiles the Rust backend. It can take several minutes. Later starts reuse Docker layers and named dependency/build-cache volumes, so they should be much faster. The app prints startup and compiler output in this terminal.

When Vite reports that it is ready, open:

```text
http://localhost:1420
```

Leave the terminal open while using Orion. Press `Ctrl+C` to stop the foreground Compose run.

### 5. Stop or restart

Stop and remove the container/network while retaining dependency caches and local project files:

```sh
docker compose down
```

Start again in the background:

```sh
docker compose up -d
```

Rebuild after changing the Dockerfile or other image build inputs:

```sh
docker compose up -d --build --force-recreate
```

View container status and logs:

```sh
docker compose ps
docker compose logs -f
```

In Docker Desktop, the same service appears under **Containers**. Its logs can be viewed there as well.

## Phone Access

The address `172.18.0.2` printed as Vite's container network address is internal to Docker. Do **not** use it on the phone. The phone needs the laptop's address on the phone-hotspot network and port `1420`.

1. Connect the laptop to the phone's hotspot and connect the phone to that same hotspot.
2. Find the laptop's IPv4 address on the hotspot interface. On Linux, `ip -brief -4 address` can help identify it; use the address assigned to the Wi-Fi/hotspot connection, not `127.0.0.1` or a `172.x.x.x` Docker address.
3. Edit `.env` and set `ORION_BIND_ADDRESS` to that exact laptop address, for example:

   ```dotenv
   ORION_BIND_ADDRESS=192.168.43.25
   ```

4. Recreate the service so Docker applies the host port binding:

   ```sh
   docker compose down
   docker compose up -d
   ```

5. On the phone, browse to `http://192.168.43.25:1420`, replacing the example IP with the laptop's actual hotspot IP.

**Phone access status:** This configuration binds the published website port to the selected host IP, but phone-to-container access has not yet been confirmed on Docker Desktop for Linux. Docker Desktop networking and the laptop firewall can prevent another device from reaching a published port even when the container is healthy. If the phone cannot connect, this V1.1 setup does not yet have verified phone support; do not change the Rust API bind address as a guess. Check the Docker Desktop networking behavior and allow inbound TCP port `1420` only on the trusted hotspot interface. Do not expose the service to the public internet. A phone hotspot reduces the set of nearby devices only if it is secured; it is not a security guarantee.

The Rust API remains on `127.0.0.1:8787` inside the same container, and Vite proxies API/media requests to it. Do not change that API bind to `0.0.0.0` for this single-container setup.

## Local Development Without Docker

The supported direct development path is Linux. Install Node.js/npm, stable Rust, Bash, curl, FFmpeg, and the Linux build libraries needed by the current Rust/Tauri dependency tree. Then from `orion-player` run:

```sh
npm install
npm run dev
```

The launcher starts the Rust media server and Vite. Open `http://localhost:1420`; press `Ctrl+C` to stop both. For frontend type-check/build:

```sh
npm run build
```

Windows and macOS native development are not verified. Docker Desktop is the intended workaround for running the Linux development environment on those systems.

## Media, Downloads, and Local Data

- The app's local media directory is `src-tauri/media/`.
- Docker bind-mounts the project folder into `/app`, so files the app writes under `/app/src-tauri/media/` are stored in the host's `orion-player/src-tauri/media/` folder and remain after the container stops.
- `.dockerignore` excludes downloaded video files from the image build context; it does not prevent the running app from writing files into the mounted project directory.
- `.gitignore` excludes downloaded video formats and generated local state from Git. Do not commit media you do not have rights to distribute.
- `ani-cli` and download tools are installed in the Docker image. Their external providers may change, rate-limit, or block requests; container setup cannot guarantee a successful download.
- The `.env` file is local and ignored by Git. Keep it that way; do not put secrets in the committed `.env.example`.

## Catalog

Edit `data/catalog.json` to describe library entries. A minimal example:

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

Keep catalog media paths consistent with files under `src-tauri/media/`. The app also exposes local API routes through Vite during development, so serving only the static Vite build is not enough for the full experience.

## Troubleshooting

**`docker: command not found`**: Install Docker Desktop or Docker Engine and Compose, start the engine, then open a new terminal.

**Cannot connect to the Docker daemon**: Make sure Docker Desktop says its engine is running and check `docker context ls`. On native Linux Engine, the user may need Docker socket permissions or to prefix Docker commands with `sudo`.

**First startup takes a long time**: A cold Rust build compiles many dependencies. Wait while new `Compiling ...` lines appear. The launcher allows up to 30 minutes and prints periodic progress. Later runs should use the Cargo target volume.

**`ani-cli` or media tool not found**: Rebuild/recreate with `docker compose up -d --build --force-recreate`, then inspect `docker compose logs -f`.

**Phone cannot open the page**: Confirm `.env` has the laptop's hotspot-interface IP, recreate the container, use that IP with port `1420`, and check the host firewall. Docker's `172.x.x.x` address is not the phone URL. Phone access through Docker Desktop for Linux remains unverified in this release.

**Port 1420 is already in use**: Stop the other service using that port before starting Orion. The Vite config currently expects port `1420`.

## AI Assistance

I built this project myself and used AI as a supporting tool for selected parts of the work. Since I am still learning Rust, I especially used it to help explain Rust concepts, explore approaches, and work through some code. The project and its direction are mine, and I continue to review, test, and learn from the code.