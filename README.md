# ANIFLEX

### Your anime library. Your machine. Your rules.

**ANIFLEX** is a locally hosted anime library built to feel like a personal Netflix , except the library lives on your own computer.

It combines a modern **React + TypeScript** interface with a **Rust** backend responsible for local media, playback, catalog management, progress tracking, and downloads.

ANIFLEX is designed around one main idea:

> **Build your own anime library instead of depending on a permanent online streaming service.**

The project can run directly on Linux or inside a container using **Podman or Docker**, making it possible to run the same development environment across different operating systems.

---

## What is ANIFLEX?

ANIFLEX is a personal, offline-first anime library application.

Instead of relying on a remote database and streaming server for every viewing session, ANIFLEX works with media stored locally on your machine.

The application consists of:

* A React/TypeScript frontend
* Vite development tooling
* A Rust backend and local HTTP media server
* Local video playback
* Subtitle support
* Local watch progress
* Anime ratings and preferences
* A recommendation/prediction system
* Automated anime downloading
* Anime discovery and metadata
* Docker/Podman container support
* Experimental local-network/phone access

The result is a self-hosted anime experience that can continue working even when you are no longer connected to the internet, provided the anime has already been downloaded.

---

# Project Overview

```text
                    ANIFLEX
                       │
        ┌──────────────┴──────────────┐
        │                             │
   React Frontend                 Rust Backend
   TypeScript                     Local HTTP API
   Vite                           Media Server
        │                             │
        └──────────────┬──────────────┘
                       │
                 Local Library
                       │
          ┌────────────┼────────────┐
          │            │            │
        Videos      Subtitles    Metadata
          │            │            │
          └────────────┴────────────┘
                       │
                Recommendation
                     Model
```

The browser communicates with the Rust server locally.

The Rust server handles the parts of the application that should not be handled purely by the frontend, including local media access and API requests.

---

# Features

## Local Anime Library

ANIFLEX stores your downloaded anime locally.

Your library isn't dependent on an ANIFLEX cloud account or a remote database.

Your local files remain on your machine.

---

## Local Video Playback

Watch downloaded episodes directly through the ANIFLEX interface.

The local Rust media server handles video requests and provides them to the frontend.

The player supports functionality such as:

* Episode playback
* Episode navigation
* Resume/continue watching
* Playback progress
* Subtitles/captions
* Playback controls
* Fullscreen playback

---

## 📚 Anime Metadata

ANIFLEX can associate downloaded anime with metadata such as:

* Title
* English title
* Romaji title
* Genres
* Format
* Episode count
* Rating
* Poster artwork
* Synopsis/information

This allows the local library to feel more like a proper streaming-service interface rather than a folder full of video files.

---

## Personal Ratings

ANIFLEX allows you to express your preferences through:

* Like
* Dislike
* Unrated

These preferences are used as part of the recommendation system.

Your library therefore isn't just a collection of downloaded files , it can become training data for your personal anime recommendations.

---

# Recommendation System

One of the more experimental parts of ANIFLEX is its recommendation/prediction system.

The goal is to make the library increasingly personalized based on what you actually watch and like.

The general workflow is:

```text
                Your Anime History
                        │
                        ▼
                Preference Data
                        │
                 ┌──────┴──────┐
                 │             │
               Liked        Disliked
                 │             │
                 └──────┬──────┘
                        ▼
                 Prediction Model
                        │
                        ▼
              New Anime Suggestions
                        │
                        ▼
                  Local Library
```

### How to start training it

A new installation may initially have an empty library.

Start by opening **Add Anime** and downloading a small number of anime.

A varied collection is preferable when beginning the recommendation process.

For example:

```text
Anime A
Anime B
Anime C
Anime D
Anime E
```

Then rate the anime you actually like.

The model can use those preferences as part of its history when deciding what should be downloaded or recommended next.

---

## Automated Downloads

ANIFLEX uses external command-line tools for its download workflow.

The container environment includes:

* `ani-cli`
* `yt-dlp`
* `ffmpeg`
* `fzf`

These tools are installed **inside the container** when using the container workflow.

That means you do not need to separately install every media tool on your computer.

> Download availability depends on external providers and tools. Providers can change, block requests, rate-limit users, or stop working entirely. ANIFLEX cannot guarantee that an external source will always remain available.

---

# Technology Stack

| Component          | Technology       |
| ------------------ | ---------------- |
| Frontend           | React            |
| Language           | TypeScript       |
| Frontend tooling   | Vite             |
| Backend            | Rust             |
| Local API          | Rust HTTP server |
| Media              | FFmpeg           |
| Downloading        | ani-cli / yt-dlp |
| Containerization   | Docker / Podman  |
| Metadata           | AniList          |
| Package management | npm / Cargo      |

---

# Project Structure

The repository contains the main ANIFLEX application inside `/orion-player`.

A simplified view:

```text
Aniflex/
│
├── orion-player/
│   │
│   ├── src/
│   │   ├── App.tsx
│   │   ├── main.tsx
│   │   └── style.css
│   │
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── ani_cli.rs
│   │   │   ├── prediction.rs
│   │   │   └── video_server.rs
│   │   │
│   │   └── media/
│   │
│   ├── public/
│   ├── data/
│   │   └── catalog.json
│   │
│   ├── Dockerfile
│   ├── docker-compose.yml
│   ├── .env.example
│   ├── package.json
│   └── vite.config.ts
│
├── test_data/
│
└── README.md
```

The exact structure can change as ANIFLEX develops.

---

# Running ANIFLEX with a Container

The easiest way to avoid installing the entire development stack manually is to use the included container environment.

ANIFLEX supports:

* **Podman**
* **Docker**

The same `Dockerfile` and Compose configuration are used for both.

The container provides the project's Linux environment, including Rust, Node.js, FFmpeg, ani-cli, yt-dlp, and other required dependencies.

---

# Windows Users

Windows users **do not need to install Rust, Node.js, FFmpeg, ani-cli, or the other ANIFLEX development dependencies directly** when using the container workflow.

Instead, use either:

### Recommended

**Docker Desktop**

or:

**Podman Desktop**

Podman on Windows requires a Linux environment through a Podman machine/WSL2 because ANIFLEX's container is a Linux environment.

### Docker Desktop

Install Docker Desktop and make sure its engine is running.

Then verify:

```powershell
docker --version
docker compose version
```

### Podman Desktop

Install Podman Desktop and configure its Podman machine.

Then verify:

```powershell
podman --version
podman-compose --version
```

If using Podman, the Podman machine must be running before starting ANIFLEX.

For example:

```powershell
podman machine start
```

---

# Linux Users

Linux users can run Podman natively without a virtual machine.

### CachyOS / Arch Linux

```bash
sudo pacman -S podman podman-compose
```

Verify:

```bash
podman --version
podman-compose --version
```

### Fedora

```bash
sudo dnf install podman podman-compose
```

### Docker

Docker Engine and Docker Compose can also be used on Linux.

---

# macOS

macOS users can use:

* Docker Desktop
* Podman Desktop

Both run Linux containers through a virtualized Linux environment.

---

# Getting ANIFLEX

Clone the repository:

```bash
git clone --branch master https://github.com/BagellXD/Aniflex.git
```

Enter the application directory:

```bash
cd Aniflex/orion-player
```

**Important:** Compose files are located inside `orion-player`.

Running the Compose command from the repository's parent directory will not work correctly.

---

# Optional Configuration

ANIFLEX does not require an `.env` file for a normal local installation.

If you want to customize the local environment, copy the example file.

### Linux / macOS / Git Bash

```bash
cp .env.example .env
```

### Windows PowerShell

```powershell
Copy-Item .env.example .env
```

For a low-memory computer, you can reduce Rust's compilation concurrency:

```env
ORION_CARGO_BUILD_JOBS=1
```

The default configuration uses two Rust compilation jobs to reduce peak memory usage.

---

# Starting ANIFLEX

## Podman

From inside `orion-player`:

```bash
podman-compose up --build
```

## Docker

```bash
docker compose up --build
```

The first build can take several minutes.

This is expected.

The first build needs to:

1. Download the container base image
2. Install Linux dependencies
3. Install Node dependencies
4. Install Rust dependencies
5. Compile the Rust backend
6. Prepare the development environment

Later launches should be significantly faster because dependencies and build artifacts are cached.

---

# Opening ANIFLEX

Once the container is running and Vite reports that it is ready, open:

```text
http://localhost:1420
```

You should now see the ANIFLEX interface.

Keep the terminal running while using the application.

Press:

```text
Ctrl + C
```

to stop the foreground process.

---

# Running in the Background

Instead of keeping the terminal attached:

### Podman

```bash
podman-compose up -d
```

### Docker

```bash
docker compose up -d
```

Check the container:

```bash
podman-compose ps
```

Follow its logs:

```bash
podman-compose logs -f
```

For Docker:

```bash
docker compose ps
docker compose logs -f
```

---

# Updating ANIFLEX

When a new ANIFLEX update is published, update the source first, then recreate the container from the `orion-player` directory.

## Update the project source

From the repository root (the directory containing `orion-player`), check for local changes and pull the latest `master` branch:

```bash
git status --short
git pull --ff-only origin master
```

If Git refuses because you have local edits, keep those edits and resolve or commit them before updating. Do not discard local catalog, playback, or media data to force an update.

## Update a Podman container

```bash
cd orion-player
podman-compose up -d --build --force-recreate
podman-compose logs -f orion
```

Check service status with `podman-compose ps`. Press `Ctrl+C` to stop following logs; this does not stop the container.

## Update a Docker container

```bash
cd orion-player
docker compose up -d --build --force-recreate
docker compose logs -f orion
```

Check service status with `docker compose ps`.

## Source-only changes

The project directory is bind-mounted into the container. Frontend edits normally hot-reload, and Rust edits are rebuilt by the development launcher. If the running app does not pick up a source/configuration change, recreate it without rebuilding the image:

```bash
# Podman
podman-compose up -d --force-recreate

# Docker
docker compose up -d --force-recreate
```

Use `--build` when the `Dockerfile`, installed tools, or image dependencies changed, or when you want to ensure the image is rebuilt from the updated source. Routine updates do not require removing the cache volumes. Avoid `down --volumes` unless you intentionally want to delete the npm/Cargo caches; local media and project files are separate, but should always be backed up.

---

# Development Workflow

ANIFLEX uses two major processes:

```text
Vite
  │
  │ Frontend
  ▼
React / TypeScript
  │
  │ HTTP requests
  ▼
Rust HTTP Server
  │
  ├── Catalog
  ├── Video
  ├── Subtitles
  ├── Progress
  └── Downloads
```

Inside the container these processes run together.

The website is exposed on:

```text
1420
```

The Rust API runs internally on:

```text
127.0.0.1:8787
```

The Rust API is intentionally not exposed directly to the host in the normal container configuration.

Vite proxies the necessary requests to it.

---

# Local Media

Downloaded media is stored inside:

```text
orion-player/src-tauri/media/
```

Because the project directory is bind-mounted into the container, files created there remain on the host computer.

Stopping or removing the container does **not** remove your anime library.

The named container volumes:

```text
orion_node_modules
orion_cargo_target
```

are dependency/build caches.

They are **not** your anime library.

---

# Your Data

The important distinction is:

```text
Project files
     │
     ├── Source code
     ├── Configuration
     └── Local application data
     
Media
     │
     └── src-tauri/media/

Container caches
     │
     ├── node_modules
     └── Cargo build artifacts
```

Removing the container does not normally delete the host project or downloaded media.

However:

```bash
podman-compose down --volumes
```

or:

```bash
docker compose down --volumes
```

removes the dependency/build cache volumes.

The next build will therefore take longer.

---

# What Should I Run After Changing Something?

### Changed React / TypeScript / CSS?

Vite normally hot-reloads the changes.

### Changed Rust?

Restart the application/container so Cargo recompiles the backend.

### Changed `.env`?

Recreate the service:

```bash
podman-compose up -d --force-recreate
```

### Changed `docker-compose.yml`?

Recreate:

```bash
podman-compose up -d --force-recreate
```

### Changed `Dockerfile`?

Rebuild:

```bash
podman-compose up -d --build --force-recreate
```

For Docker, replace `podman-compose` with:

```bash
docker compose
```

---

# Cleaning the Environment

If you want to stop the application while keeping dependency caches:

```bash
podman-compose down
```

If you intentionally want to remove the dependency/build cache volumes:

```bash
podman-compose down --volumes
```

Be aware that the next startup will need to recreate those caches.

---

# Phone / Local Network Access

ANIFLEX can potentially be accessed from another device on the same local network.

This functionality is currently considered **experimental**.

The important distinction is:

```text
127.0.0.1
```

is your computer itself.

And an address such as:

```text
172.x.x.x
```

may be an internal container-network address.

Your phone should instead connect to the **computer's LAN/hotspot IP address**.

For example:

```text
http://192.168.43.25:1420
```

The exact address will depend on your network.

---

## Network Security

Do not expose the ANIFLEX development server directly to the public internet.

If enabling LAN access:

* Use a trusted network
* Keep the host firewall enabled
* Only allow the required port
* Do not expose the Rust API unnecessarily
* Do not treat a phone hotspot as a complete security boundary

Phone access through every Podman/Docker networking configuration is not currently guaranteed.

---

# Running Without Docker/Podman

Direct development is currently intended primarily for Linux.

You will need the appropriate development tools, including:

* Node.js
* npm
* Rust
* FFmpeg
* Bash
* curl
* Required Linux build libraries

Then:

```bash
npm install
```

and:

```bash
npm run dev
```

The development launcher starts the Rust media server and Vite.

Open:

```text
http://localhost:1420
```

For a frontend build:

```bash
npm run build
```

Native Windows/macOS development is not the primary tested workflow.

For those platforms, the container environment is recommended.

---

# Catalog

ANIFLEX maintains local library metadata through:

```text
data/catalog.json
```

A simplified entry looks like:

```json
[
  {
    "id": "sample-show",
    "title": "Sample Show",
    "year": 2024,
    "format": "TV",
    "genres": [
      "Adventure",
      "Fantasy"
    ],
    "synopsis": "A short local-library description.",
    "poster": "/media/sample-show-poster.jpg",
    "video": "/media/sample-show.mp4"
  }
]
```

Media paths must correspond to files that actually exist in the local media directory.

---

# External Download Dependencies

ANIFLEX's download workflow depends on external software and providers.

The container includes:

```text
ani-cli
yt-dlp
ffmpeg
fzf
```

You can verify them inside the running container:

```bash
podman-compose exec orion sh
```

Then:

```bash
command -v ani-cli
command -v yt-dlp
command -v ffmpeg
```

If those commands return executable paths, the tools are available inside the container.

---

# Troubleshooting

## `podman: command not found`

Podman is not installed or is not available in your PATH.

Install Podman for your operating system and open a new terminal.

---

## `podman-compose: command not found`

Install the Compose provider.

On Arch/CachyOS:

```bash
sudo pacman -S podman-compose
```

Then:

```bash
podman-compose --version
```

---

## Windows says Podman is not recognized

Make sure Podman/Podman Desktop is actually installed.

Then verify:

```powershell
podman --version
```

If Podman is installed but the command is unavailable, restart the terminal.

If using Podman Desktop, also make sure the Podman machine has been initialized and started.

---

## `podman machine` problems on Windows

Check:

```powershell
podman machine list
```

Then start the machine:

```powershell
podman machine start
```

If no machine exists:

```powershell
podman machine init
```

Then:

```powershell
podman machine start
```

---

## `no configuration file provided`

You are probably running Compose from the wrong directory.

Make sure you are inside:

```text
Aniflex/orion-player
```

Then run:

```bash
podman-compose up --build
```

---

## The first build looks frozen

Rust compilation can take a while, particularly on lower-end hardware.

Look for lines such as:

```text
Compiling ...
```

The first build is substantially slower than later launches.

Do not immediately assume that the process has crashed.

---

## Build uses too much RAM

Create `.env`:

```bash
cp .env.example .env
```

Then set:

```env
ORION_CARGO_BUILD_JOBS=1
```

This reduces parallel Rust compilation and therefore reduces peak memory usage.

The trade-off is a longer build.

---

## `Failed to start ani-cli`

The container may have been built from an older image.

Rebuild it:

```bash
podman-compose up -d --build --force-recreate
```

Then inspect:

```bash
podman-compose logs -f
```

---

## `No player found. Looked for mpv and vlc`

ANIFLEX's download workflow requires the configured download player.

Check:

```bash
podman-compose exec orion sh -lc 'printf "ANI_CLI_PLAYER=%s\n" "$ANI_CLI_PLAYER"; command -v ani-cli; command -v ffmpeg; command -v yt-dlp'
```

The environment should contain:

```text
ANI_CLI_PLAYER=ffmpeg
```

and the commands should locate:

```text
ani-cli
ffmpeg
yt-dlp
```

If they do not:

```bash
podman-compose up -d --build --force-recreate
```

---

## ANIFLEX starts and then stops

Check the service:

```bash
podman-compose ps
```

Then inspect the logs:

```bash
podman-compose logs --tail=100 orion
```

For live logs:

```bash
podman-compose logs -f orion
```

---

## Port `1420` is already in use

Another application is already using ANIFLEX's Vite port.

Stop the application using the port before starting ANIFLEX.

The current development configuration expects:

```text
1420
```

---

## Phone cannot connect

Check:

1. The phone and computer are on the same network.
2. You are using the computer's LAN/hotspot IP.
3. You are using port `1420`.
4. The container has been recreated after changing the bind address.
5. The host firewall allows the connection.

Do **not** use the container's `172.x.x.x` address from the phone.

---

# Why Rust?

The backend was built with Rust because ANIFLEX is intended to do more than simply display a webpage.

Rust handles the lower-level parts of the application, including:

* Local HTTP serving
* File handling
* Media-related operations
* Download orchestration
* Application state
* Recommendation logic
* Process management

The frontend is responsible for the visual experience, while Rust handles much of the underlying application logic.

This separation also makes ANIFLEX an ongoing learning project for systems programming and Rust development.

---

# Architecture

At a high level:

```text
┌─────────────────────────────────────────┐
│                ANIFLEX UI                │
│                                         │
│          React + TypeScript             │
│                 + Vite                  │
└───────────────────┬─────────────────────┘
                    │
                    │ HTTP
                    ▼
┌─────────────────────────────────────────┐
│              Rust Backend               │
│                                         │
│           Local HTTP Server              │
│                                         │
│  Catalog │ Playback │ Progress │ Media  │
│                                         │
│        Download / Prediction            │
└───────────┬───────────────┬─────────────┘
            │               │
            ▼               ▼
      Local Library     External Tools
            │          ani-cli / yt-dlp
            │               │
            ▼               ▼
      Video / Subs       Downloads
```

When running through Compose, these components run together inside one container.

---

# Privacy & Local-First Design

ANIFLEX is designed around local storage.

The core library, downloaded media, playback state, and application data are intended to remain on the user's machine.

The application does not require a centralized ANIFLEX account to operate its local library.

However, external services and download providers may still be contacted when you search for metadata or download anime.

Always understand what external services a tool is contacting before using it.

---

# Important Disclaimer

ANIFLEX is a personal software project and development environment.

It does not host or distribute a centralized anime catalog.

Anime availability depends on external tools and providers, which can change independently of ANIFLEX.

Users are responsible for complying with the laws, licenses, copyright rules, and terms of service applicable to the content they access, download, store, or distribute.

Do not commit or distribute copyrighted media through this repository.

---

# 🤖 AI-Assisted Development

ANIFLEX was built as a personal learning project.

AI tools have been used as development assistance for selected tasks, including:

* Explaining programming concepts
* Exploring implementation approaches
* Debugging
* Reviewing code
* Working through Rust concepts

The project direction, architecture, experimentation, and development remain part of the author's work.

The purpose of using AI here is not simply to generate code and move on, but to use it as a tool for learning and solving difficult engineering problems.

---

# Project Direction

ANIFLEX is still evolving.

The project is being developed around several long-term goals:

* Better local playback
* More reliable downloading
* Better recommendation quality
* Stronger Rust architecture
* Improved media organization
* Better cross-platform support
* Improved local-network support
* More robust metadata handling
* More polished UI/UX
* Less dependence on manual configuration

The architecture is intentionally being developed incrementally rather than attempting to build the entire system at once.

---

# Development Philosophy

ANIFLEX is both an application and a learning project.

The project is being used to explore:

```text
React
   ↓
TypeScript
   ↓
HTTP / APIs
   ↓
Rust
   ↓
Process management
   ↓
File systems
   ↓
Media servers
   ↓
Containers
   ↓
Recommendation systems
```

The goal is not just to make ANIFLEX work.

The goal is to understand **why it works**.

---

# Getting Started

The shortest path for a new user is:

### 1. Clone

```bash
git clone --branch master https://github.com/BagellXD/Aniflex.git
```

### 2. Enter the application

```bash
cd Aniflex/orion-player
```

### 3. Install a container runtime

Choose:

* Docker
* Podman

### 4. Start ANIFLEX

Podman:

```bash
podman-compose up --build
```

Docker:

```bash
docker compose up --build
```

### 5. Open the application

```text
http://localhost:1420
```

### 6. Start building your library

Open **Add Anime**, download some anime, watch them, rate them, and allow the recommendation system to build from your preferences.

---

# Current Status

**ANIFLEX is an active personal project.**

Some components are mature enough for regular use, while others are still experimental.

In particular:

* Container-based development is supported.
* Linux development is the primary native development environment.
* Windows/macOS users can use the container workflow.
* Local phone access is experimental.
* External download providers are outside the project's control.
* The recommendation system is actively evolving.

Expect changes as the project continues to develop.

---

# Built for the love of anime

ANIFLEX started as an experiment in building a personal anime experience from the ground up.

It became a way to combine:

**software engineering + Rust + React + media systems + automation + recommendation systems**

into one project.

No massive infrastructure.

No cloud account required for the core library.

Just your machine, your library, and a ridiculous amount of anime.

---

## Repository

**ANIFLEX**

https://github.com/BagellXD/Aniflex

---

### Made by BagellXD
