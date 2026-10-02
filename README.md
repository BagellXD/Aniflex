# Orion Player

**Version 1.1.0 - Forced Patch**

Orion Player is a locally hosted anime library website. The browser UI uses React, TypeScript, and Vite. A Rust HTTP server handles the local catalog, playback, progress, and API requests. The container workflow runs both processes together in a Linux container.

This is a personal project and a development setup, not a public production service. Streaming/download availability depends on external sites and tools and can change at any time.

## What Changed in 1.1

- Added a container-based Linux development environment, runnable with Podman or Docker.
- Added `ani-cli`, `yt-dlp`, FFmpeg, and `fzf` to the container for the app's download flow.
- Limited concurrent Rust compilation to two jobs by default to reduce peak build memory.
- Kept the Rust API on container loopback; only the Vite website port is published.
- Added a longer startup wait for the first Rust compile.
- Added phone-over-hotspot instructions. Phone access through rootless Podman and Docker Desktop for Linux is **not yet verified**; see the phone section below.

## Run with a Container

You can run Orion with either Docker Compose or Podman Compose. They use the same `Dockerfile` and `docker-compose.yml`; switching runtimes does not require changing Orion's code. Install one runtime, clone the repo, open a terminal in `orion-player`, and run the matching Compose command below.

**Recommended for lower-end Linux computers: Podman.** Native rootless Podman runs directly on Linux and avoids the always-on Linux virtual machine used by Docker Desktop. Docker is also supported and may be the easier choice if you already have it installed. On Windows and macOS, both Docker Desktop and Podman Desktop run Linux containers inside a virtual machine, so Podman is not automatically lighter there. OrbStack is another macOS-only runtime, but it is not required for Orion.

The container provides Node.js, Rust, Linux build libraries, `ani-cli`, `yt-dlp`, FFmpeg, and other required tools. You do **not** need to install those project dependencies on your computer. The first build downloads the base image and dependencies and compiles the Rust backend; expect it to take several minutes (sometimes longer on a low-end machine). Later starts reuse cached dependencies and are much quicker. Container setup is straightforward, but the first build is not instant.

### 1. Install a container runtime

Choose one option:

- **Linux, lower memory use:** Install Podman and the Compose provider. On CachyOS/Arch, run `sudo pacman -S podman podman-compose`. On Fedora, use `sudo dnf install podman podman-compose`. On Debian/Ubuntu, install `podman` and `podman-compose` from the distribution's package manager; package availability can depend on the release. You do not need to start a Podman daemon or initialize a Podman machine on Linux. Podman Desktop is optional; the engine and Compose provider are what run the app.
- **Windows or macOS:** Install Docker Desktop, start it, and wait for its engine to be ready. Podman Desktop is also an option, but on these operating systems it needs a Podman machine (a Linux VM) to run Linux containers.
- **Linux with Docker:** Docker Engine and the Docker Compose plugin work too. Docker Desktop also works, but includes a Linux VM and can use more baseline memory than native Podman.

Use the official installation guides for the runtime you choose: [Podman installation](https://podman.io/docs/installation), [Podman Desktop](https://podman-desktop.io/docs/installation), or [Docker Desktop](https://docs.docker.com/get-started/get-docker/). On Linux, prefer your distribution's packages for Podman.

Check the commands for your selected runtime:

```sh
# Podman
podman --version
podman-compose --version

# Or Docker
docker --version
docker compose version
```

For Podman, use the `podman-compose` command shown below. `podman compose` is a wrapper that selects an installed provider; on some computers it may accidentally select a Docker Compose plugin instead. Using `podman-compose` directly avoids that ambiguity.

### 2. Download the project

If you do not already have the repository, clone it and enter the app directory:

```sh
git clone https://github.com/BagellXD/Aniflex.git
cd Aniflex/orion-player
```

If you already have the project, open a terminal in its `orion-player` directory. This matters: running Compose from the repository's parent folder produces `no configuration file provided` because `docker-compose.yml` is inside `orion-player`.

### 3. (Optional) Set local options

No `.env` file is required for a local run. The defaults bind the website to `127.0.0.1` (your own computer) and limit Rust compilation to two jobs. To create a local settings file, copy the example:

```sh
# Linux, macOS, or Git Bash
cp .env.example .env
```

```powershell
# Windows PowerShell
Copy-Item .env.example .env
```

On a low-memory computer, put `ORION_CARGO_BUILD_JOBS=1` in `.env` before the first build. This lowers peak compile memory at the cost of a longer build. Keep the default `ORION_BIND_ADDRESS=127.0.0.1` unless you specifically need access from another device; see [Phone Access](#phone-access).

### 4. Build and start Orion

Make sure the terminal's current directory is `orion-player`, then run exactly one command for your runtime:

```sh
# Podman on Linux
podman-compose up --build
```

```sh
# Docker
docker compose up --build
```

Compose builds the image, creates the app container and its cache volumes, and starts the Rust API and Vite website together. The Rust/Node/media dependencies are installed in the image, not on your host. Watch the terminal output; when Vite says it is ready, open:

```text
http://localhost:1420
```

Keep the terminal open while using Orion. Press `Ctrl+C` to stop the foreground app.

### 5. Stop, restart, or inspect it

Use the same runtime command prefix you chose above (`podman-compose` or `docker compose`):

```sh
# Stop the app and remove its container/network; keep caches and your project files.
podman-compose down

# Start in the background instead of keeping the terminal attached.
podman-compose up -d

# See status and follow logs.
podman-compose ps
podman-compose logs -f
```

For Docker, replace `podman-compose` in those commands with `docker compose`. To rebuild after changing the Dockerfile or needing updated image tools, run `podman-compose up -d --build --force-recreate` (or the Docker equivalent).

Podman and Docker keep separate images and caches; they do not share downloaded build layers. The source project and downloaded media remain on your computer because the project folder is bind-mounted into the container. `podman-compose down --volumes` (or `docker compose down --volumes`) also deletes the dependency/build cache volumes, so the next build has to recreate them. It does not delete the host project folder or its downloaded media.

### What runs in the container?

There is one service named `orion`, not separate frontend and backend containers:

- Vite serves the website on port `1420`. The Compose file publishes that port on your computer.
- The Rust API server listens on `127.0.0.1:8787` inside the same container. Vite forwards `/api`, `/video`, and `/subtitles` requests to it; the API port is not published directly.
- The Compose bind mount maps the `orion-player` folder on your computer into `/app`. Code edits and files downloaded under `/app/src-tauri/media/` are therefore in your project folder and remain after the container stops.
- `orion_node_modules` and `orion_cargo_target` are named cache volumes for npm and Cargo dependencies/build output. They are not the anime library.
- The Dockerfile installs `ani-cli`, `yt-dlp`, FFmpeg, and `fzf` in the image. You do not need to separately install those tools on the host for the container workflow.

The `.dockerignore` file keeps local files out of the image build context. It does not prevent the running app from writing to the bind-mounted project folder.

### Which command after a change?

- Changed frontend files? Vite generally reloads them automatically.
- Changed Rust files? Restart the service so Cargo recompiles and restarts the Rust API.
- Changed `.env` or `docker-compose.yml`? Recreate the service with `podman-compose up -d --force-recreate` or `docker compose up -d --force-recreate`.
- Changed the `Dockerfile` or need a newly installed tool? Rebuild with `podman-compose up -d --build --force-recreate` or `docker compose up -d --build --force-recreate`.
- Want to stop but keep caches? Use the matching `down` command without `--volumes`.
- Want to clear caches too? Use the matching `down --volumes` command; the next startup will rebuild caches.

## Phone Access

The address `172.18.0.2` printed as Vite's container network address is internal to the container network. Do **not** use it on the phone. The phone needs the laptop's address on the phone-hotspot network and port `1420`.

1. Connect the laptop to the phone's hotspot and connect the phone to that same hotspot.
2. Find the laptop's IPv4 address on the hotspot interface. On Linux, `ip -brief -4 address` can help identify it; use the address assigned to the Wi-Fi/hotspot connection, not `127.0.0.1` or a `172.x.x.x` Docker address.
3. Edit `.env` and set `ORION_BIND_ADDRESS` to that exact laptop address, for example:

   ```dotenv
   ORION_BIND_ADDRESS=192.168.43.25
   ```

4. Recreate the service so the container runtime applies the host port binding:

   ```sh
  podman-compose down
  podman-compose up -d
   ```

5. On the phone, browse to `http://192.168.43.25:1420`, replacing the example IP with the laptop's actual hotspot IP.

**Phone access status:** This configuration binds the published website port to the selected host IP, but phone-to-container access has not yet been confirmed with rootless Podman or Docker Desktop for Linux. Container networking and the laptop firewall can prevent another device from reaching a published port even when the container is healthy. If the phone cannot connect, this V1.1 setup does not yet have verified phone support; do not change the Rust API bind address as a guess. Check the runtime's networking behavior and allow inbound TCP port `1420` only on the trusted hotspot interface. Do not expose the service to the public internet. A phone hotspot reduces the set of nearby devices only if it is secured; it is not a security guarantee.

The Rust API remains on `127.0.0.1:8787` inside the same container, and Vite proxies API/media requests to it. Do not change that API bind to `0.0.0.0` for this single-container setup.

## How the Recommendation Model Works

When you first open the website, it will appear empty. To begin, go to **Add Anime**, search for anime you want, and download them.

This is the main way to train the recommendation model: the app learns from the anime you add and rate. It is best to download up to 5 anime at a time, ideally 5 different titles. You can download the first episode of each anime or choose a specific episode, but a small set of varied anime works best.

After each download, wait for the page to reload on its own. That reload means the anime has finished downloading and the app is ready for the next one. Download speed depends on your network connection.

To make the recommendation system work properly, you must like the anime you want it to learn from. To do that, click on a video and look at the top-right of the player. There you will see **Like** and **Dislike** buttons. If you like an anime, the model uses it as positive history. If you dislike it, or simply do not give it a like, the model treats it as weak or negative input.

The prediction model works by downloading the next episode of each liked anime, then downloading 5 other recommended anime in addition to that. This means the model keeps building from the anime you liked and continues the story from there. If an anime is not liked, similar anime and their next episodes are less likely to be downloaded.

This is also how the app learns from your taste: if you dislike an anime or do not like it, similar titles and their next episodes will not be downloaded as often. That gives the model a cleaner history to work with.

You can increase how many other anime the model downloads and raise the total anime cap in the source code. These are controlled by constants in the Rust source, so if you want more recommendations or a larger library cap, you can tune those values there.

Another way to give the prediction model history is to use the `ani-cli` history and input anime names in this format:

```text
{anime name and its episode}[general anime name]
```

This helps the model build a stronger history using the titles and episode pattern you have already watched and liked.

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

Windows and macOS native development are not verified. Docker Desktop or Podman Desktop can run the Linux container environment on those systems.

## Media, Downloads, and Local Data

- The app's local media directory is `src-tauri/media/`.
- The Compose bind mount maps the project folder into `/app`, so files the app writes under `/app/src-tauri/media/` are stored in the host's `orion-player/src-tauri/media/` folder and remain after the container stops.
- `.dockerignore` excludes downloaded video files from the image build context; it does not prevent the running app from writing files into the mounted project directory.
- `.gitignore` excludes downloaded video formats and generated local state from Git. Do not commit media you do not have rights to distribute.
- `ani-cli` and download tools are installed in the container image. Their external providers may change, rate-limit, or block requests; container setup cannot guarantee a successful download.
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

**`podman: command not found`**: Install Podman and a Compose provider from your Linux distribution, then open a new terminal. On Linux, Podman does not require a background daemon or Podman machine.

**`podman-compose: command not found`**: Install the `podman-compose` package from your distribution. For example, on CachyOS/Arch use `sudo pacman -S podman-compose`; on Fedora use `sudo dnf install podman-compose`.

**`podman compose` unexpectedly runs Docker Compose**: Use `podman-compose` directly for the commands in this guide. `podman compose` delegates to a provider and may select a Docker Compose plugin installed under `~/.docker`.

**First startup takes a long time**: A cold Rust build compiles many dependencies. Wait while new `Compiling ...` lines appear. The launcher allows up to 30 minutes and prints periodic progress. Later runs should use the Cargo target volume.

**Download says `Failed to start ani-cli: No such file or directory`**: The running container may have been created from an older image without the executable. From `orion-player`, run `podman-compose up -d --build --force-recreate` (or `docker compose up -d --build --force-recreate`), then check the service logs.

**Download says `No player found. Looked for mpv and vlc`**: This app invokes ani-cli in download mode. The Compose service sets `ANI_CLI_PLAYER=ffmpeg`, and the Docker image installs FFmpeg; both are needed because ani-cli checks for a player before it handles its download option. Confirm the current configuration and binaries with:

```sh
podman-compose config
podman-compose exec orion sh -lc 'printf "ANI_CLI_PLAYER=%s\\n" "$ANI_CLI_PLAYER"; command -v ani-cli; command -v ffmpeg; command -v yt-dlp'
```

The config should show `ANI_CLI_PLAYER: ffmpeg`, and the commands should print paths for `ani-cli`, `ffmpeg`, and `yt-dlp`. If not, rebuild/recreate with `podman-compose up -d --build --force-recreate`. If all tools are present but the download still fails, inspect `podman-compose logs -f`; the external anime provider or ani-cli itself may be failing, which is separate from the container finding the executable.

**Need to see why startup stopped**: Run `podman-compose ps` first. If the service exited, inspect its logs with `podman-compose logs --tail=100 orion`. For live output while reproducing a problem, use `podman-compose logs -f orion` and press `Ctrl+C` to stop following logs; this does not stop the container. For Docker, replace `podman-compose` with `docker compose`.

**Phone cannot open the page**: Confirm `.env` has the laptop's hotspot-interface IP, recreate the container, use that IP with port `1420`, and check the host firewall. A container's `172.x.x.x` address is not the phone URL. Phone access through rootless Podman and Docker Desktop for Linux remains unverified in this release.

**Port 1420 is already in use**: Stop the other service using that port before starting Orion. The Vite config currently expects port `1420`.

## AI Assistance

I built this project myself and used AI as a supporting tool for selected parts of the work. Since I am still learning Rust, I especially used it to help explain Rust concepts, explore approaches, and work through some code. The project and its direction are mine, and I continue to review, test, and learn from the code.
