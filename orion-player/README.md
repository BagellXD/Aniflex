# Orion Player

An offline-first desktop library prototype built with Tauri, React, and TypeScript.

## Run

From this directory:

```sh
npm install
npm run tauri dev
```

Linux builds of Tauri require the platform's WebKitGTK development packages. The frontend can also be previewed in a browser with `npm run dev` after installing dependencies.

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

Poster and video paths are local app paths; media files are ignored by Git. Playback and prediction integration are not wired yet.