use std::{
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    thread,
};

use serde::Deserialize;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const PORT: u16 = 8787;

static GENERATION_RUNNING: AtomicBool = AtomicBool::new(false);
static MANUAL_DOWNLOAD_RUNNING: AtomicBool = AtomicBool::new(false);
static SERVER_STARTED: OnceLock<()> = OnceLock::new();

static PLAYBACK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

// Serializes like/dislike work (catalog.json + history files) now that
// requests are handled on separate threads.
static HISTORY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct HistoryRequest {
    id: String,
    title: String,
    episode: u32,
}

#[derive(Debug, Deserialize)]
struct AnimeSearchRequest {
    query: String,
}

#[derive(Debug, Deserialize)]
struct ManualDownloadRequest {
    title: String,
    episode: u32,
    total_episodes: Option<u32>,
    poster_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProgressRequest {
    id: String,
    episode: u32,
    position: f64,
}

// ============================================================
// SERVER START
// ============================================================

pub fn start() {
    if SERVER_STARTED.set(()).is_err() {
        return;
    }

    println!("MEDIA SERVER STARTING");
    println!("Media root: {}", media_root().display());

    let server = match Server::http(("127.0.0.1", PORT)) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("Failed to start media server: {error}");
            return;
        }
    };

    println!("Media server running at http://127.0.0.1:{PORT}");

    for request in server.incoming_requests() {
        // Each request gets its own thread so that large video responses
        // do not block API requests, subtitles, progress saves, etc.
        thread::spawn(move || handle_request(request));
    }
}

// ============================================================
// REQUEST ROUTING
// ============================================================

fn handle_request(request: Request) {
    let method = request.method().clone();

    // Owned copies so nothing borrows `request` when it is moved
    // into a handler.
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("").to_string();

    if method == Method::Options {
        respond_empty(request, StatusCode(204));
        return;
    }

    match (method, path.as_str()) {
        // ---------------------- API ----------------------
        (Method::Get, "/api/catalog") => handle_catalog(request),
        (Method::Get, "/api/library/count") => handle_library_count(request),
        (Method::Get, "/api/generate/status") => handle_generate_status(request),
        (Method::Post, "/api/generate") => handle_generate(request),
        (Method::Post, "/api/anime/search") => handle_anime_search(request),
        (Method::Post, "/api/anime/download") => handle_manual_anime_download(request),
        (Method::Post, "/api/history/like") => handle_history_change(request, true),
        (Method::Post, "/api/history/dislike") => handle_history_change(request, false),
        (Method::Get, path) if path.starts_with("/api/progress/") => {
            handle_get_progress(request, path)
        }
        (Method::Post, "/api/progress") => handle_save_progress(request),

        // ---------------------- MEDIA --------------------
        (Method::Get, path) if path.starts_with("/video/") => handle_media(request, true),
        (Method::Get, path) if path.starts_with("/media/") => handle_media(request, false),
        (Method::Get, path) if path.starts_with("/subtitles/") => handle_subtitles(request),

        _ => respond_text(request, StatusCode(404), "Not found"),
    }
}

// ============================================================
// API: CATALOG
// ============================================================

fn handle_catalog(request: Request) {
    match fs::read_to_string(catalog_path()) {
        Ok(content) => {
            let body = enrich_catalog(&content);
            respond_json(request, StatusCode(200), &body);
        }
        Err(error) => respond_text(
            request,
            StatusCode(500),
            &format!("Failed to read catalog: {error}"),
        ),
    }
}

// ============================================================
// CATALOG ENRICHMENT
//
// catalog.json (written by the downloader) lists episodes as
//     { "episode": 3 }
// but the player needs
//     { "number": 3, "video": "<file name on disk>", "subtitle": "..." }
// so /api/catalog looks at the files in media/<id>/ and fills that in.
// ============================================================

const VIDEO_EXTENSIONS: [&str; 6] = ["mp4", "m4v", "webm", "mkv", "mov", "avi"];

fn file_extension(path: &std::path::Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn digits_after_separators(text: &str) -> Option<u32> {
    let rest = text.trim_start_matches(|c: char| matches!(c, ' ' | '_' | '-' | '.' | '#' | ':'));

    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();

    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn episode_number_from_name(stem: &str) -> Option<u32> {
    let lower = stem.to_lowercase();

    // "Episode 12", "episode_12", "ep12"
    for marker in ["episode", "ep"] {
        let mut from = 0;

        while let Some(found) = lower[from..].find(marker) {
            let at = from + found;
            let after = at + marker.len();

            let boundary = lower[..at]
                .chars()
                .next_back()
                .map_or(true, |c| !c.is_alphabetic());

            if boundary {
                if let Some(number) = digits_after_separators(&lower[after..]) {
                    return Some(number);
                }
            }

            from = after;
        }
    }

    // "s01e05"
    let chars: Vec<char> = lower.chars().collect();

    for index in 1..chars.len() {
        if chars[index] == 'e' && chars[index - 1].is_ascii_digit() {
            let digits: String = chars[index + 1..]
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .collect();

            if let Ok(number) = digits.parse::<u32>() {
                return Some(number);
            }
        }
    }

    // Last short run of digits, e.g. "Show - 07" (ignores 1080, 2160, years)
    let mut best: Option<u32> = None;
    let mut run = String::new();

    for c in lower.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_digit() {
            run.push(c);
        } else {
            if !run.is_empty() && run.len() <= 3 {
                if let Ok(number) = run.parse::<u32>() {
                    best = Some(number);
                }
            }
            run.clear();
        }
    }

    best
}

fn scan_episodes(anime_id: &str) -> Vec<Value> {
    let directory = media_root().join(anime_id);

    let Ok(entries) = fs::read_dir(&directory) else {
        return Vec::new();
    };

    // episode number -> (extension rank, file name); lower rank wins (mp4 first)
    let mut videos: std::collections::BTreeMap<u32, (usize, String)> =
        std::collections::BTreeMap::new();
    let mut subtitles: std::collections::BTreeMap<u32, (usize, String)> =
        std::collections::BTreeMap::new();

    for entry in entries.flatten() {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let Some(name) = path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_string)
        else {
            continue;
        };

        // Skip hidden files and ffmpeg's half-written temp files.
        if name.starts_with('.') || name.contains(".optimized.") {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };

        let extension = file_extension(&path);

        if let Some(rank) = VIDEO_EXTENSIONS.iter().position(|value| *value == extension) {
            if let Some(number) = episode_number_from_name(stem) {
                let keep_existing =
                    matches!(videos.get(&number), Some((best, _)) if *best <= rank);

                if !keep_existing {
                    videos.insert(number, (rank, name));
                }
            }
        } else if extension == "vtt" || extension == "srt" {
            if let Some(number) = episode_number_from_name(stem) {
                let rank = if extension == "vtt" { 0 } else { 1 };
                let keep_existing =
                    matches!(subtitles.get(&number), Some((best, _)) if *best <= rank);

                if !keep_existing {
                    subtitles.insert(number, (rank, name));
                }
            }
        }
    }

    videos
        .into_iter()
        .map(|(number, (_, video))| {
            let mut episode = json!({ "number": number, "video": video });

            if let Some((_, subtitle)) = subtitles.get(&number) {
                episode["subtitle"] = Value::String(subtitle.clone());
            }

            episode
        })
        .collect()
}

fn enrich_catalog(raw: &str) -> String {
    let Ok(mut catalog) = serde_json::from_str::<Value>(raw) else {
        return raw.to_string();
    };

    if let Some(items) = catalog.as_array_mut() {
        for item in items.iter_mut() {
            let Some(id) = item.get("id").and_then(Value::as_str).map(str::to_string) else {
                continue;
            };

            if id.is_empty() || contains_path_separator(&id) {
                continue;
            }

            let scanned = scan_episodes(&id);

            // Entries that already list real video file names (hand-written
            // ones) are left alone if nothing was found on disk.
            let already_has_videos = item
                .get("episodes")
                .and_then(Value::as_array)
                .map(|episodes| {
                    episodes.iter().any(|episode| {
                        episode
                            .get("video")
                            .and_then(Value::as_str)
                            .map(|video| !video.is_empty())
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);

            if scanned.is_empty() && already_has_videos {
                continue;
            }

            if let Some(object) = item.as_object_mut() {
                object.insert("episodes".to_string(), Value::Array(scanned));
            }
        }
    }

    serde_json::to_string(&catalog).unwrap_or_else(|_| raw.to_string())
}

// ============================================================
// API: LIBRARY COUNT
// ============================================================

fn handle_library_count(request: Request) {
    let count = match count_anime_directories() {
        Ok(count) => count,
        Err(error) => {
            respond_text(
                request,
                StatusCode(500),
                &format!("Failed to count library: {error}"),
            );
            return;
        }
    };

    let body = json!({ "count": count }).to_string();
    respond_json(request, StatusCode(200), &body);
}

// ============================================================
// API: GENERATION STATUS
// ============================================================

fn handle_generate_status(request: Request) {
    let body = json!({
        "running": GENERATION_RUNNING.load(Ordering::Acquire)
    })
    .to_string();

    respond_json(request, StatusCode(200), &body);
}

fn handle_anime_search(mut request: Request) {
    let mut body = String::new();
    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        respond_text(request, StatusCode(400), &format!("Failed to read request body: {error}"));
        return;
    }

    let payload: AnimeSearchRequest = match serde_json::from_str(&body) {
        Ok(payload) => payload,
        Err(error) => {
            respond_text(request, StatusCode(400), &format!("Invalid JSON: {error}"));
            return;
        }
    };

    if payload.query.trim().len() < 2 {
        respond_json(request, StatusCode(200), "[]");
        return;
    }

    match crate::ani_cli::search_anime(&payload.query) {
        Ok(results) => respond_json(request, StatusCode(200), &results.to_string()),
        Err(error) => respond_text(request, StatusCode(502), &error),
    }
}

fn handle_manual_anime_download(mut request: Request) {
    let mut body = String::new();
    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        respond_text(request, StatusCode(400), &format!("Failed to read request body: {error}"));
        return;
    }

    let payload: ManualDownloadRequest = match serde_json::from_str(&body) {
        Ok(payload) => payload,
        Err(error) => {
            respond_text(request, StatusCode(400), &format!("Invalid JSON: {error}"));
            return;
        }
    };

    let title = payload.title.trim();
    if title.is_empty() || title.len() > 200 || payload.episode == 0 {
        respond_text(request, StatusCode(400), "Invalid anime title or episode");
        return;
    }
    if payload.total_episodes.is_some_and(|total| payload.episode > total) {
        respond_text(request, StatusCode(400), "Episode exceeds the anime's episode count");
        return;
    }
    if MANUAL_DOWNLOAD_RUNNING.swap(true, Ordering::AcqRel) {
        respond_text(request, StatusCode(409), "A manual download is already running");
        return;
    }

    let title = title.to_string();
    let episode = payload.episode;
    let total_episodes = payload.total_episodes;
    let poster_url = payload.poster_url;
    thread::spawn(move || {
        let result = std::panic::catch_unwind(|| {
            crate::ani_cli::download_manual_episode(
                &title,
                episode,
                total_episodes,
                poster_url.as_deref(),
            )
        });
        match result {
            Ok(true) => println!("Manual download completed: {title} episode {episode}"),
            Ok(false) => eprintln!("Manual download failed: {title} episode {episode}"),
            Err(error) => eprintln!("Manual download panicked: {error:?}"),
        }
        MANUAL_DOWNLOAD_RUNNING.store(false, Ordering::Release);
    });

    respond_json(request, StatusCode(202), "{\"started\":true}");
}

// ============================================================
// API: GENERATE MORE
// ============================================================

fn handle_generate(request: Request) {
    if GENERATION_RUNNING.swap(true, Ordering::AcqRel) {
        respond_text(request, StatusCode(409), "Generation is already running");
        return;
    }

    let body = json!({ "started": true }).to_string();
    respond_json(request, StatusCode(202), &body);

    thread::spawn(|| {
        let result = std::panic::catch_unwind(|| {
            let _ = crate::ani_cli::download_predicted_anime();
        });

        if let Err(error) = result {
            eprintln!("Generate More panicked: {error:?}");
        }

        GENERATION_RUNNING.store(false, Ordering::Release);
    });
}

// ============================================================
// API: LIKE / DISLIKE
// ============================================================

fn handle_history_change(mut request: Request, liked: bool) {
    let mut body = String::new();

    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        respond_text(
            request,
            StatusCode(400),
            &format!("Failed to read request body: {error}"),
        );
        return;
    }

    let payload: HistoryRequest = match serde_json::from_str(&body) {
        Ok(payload) => payload,
        Err(error) => {
            respond_text(request, StatusCode(400), &format!("Invalid JSON: {error}"));
            return;
        }
    };

    let result = {
        let _guard = history_lock().lock().unwrap_or_else(|e| e.into_inner());

        if liked {
            like_history(&payload)
        } else {
            dislike_history(&payload)
        }
    };

    match result {
        Ok(()) => {
            let body = json!({ "ok": true }).to_string();
            respond_json(request, StatusCode(200), &body);
        }
        Err(error) => respond_text(request, StatusCode(500), &error),
    }
}

// ============================================================
// API: GET PLAYBACK PROGRESS
//
// GET /api/progress/<anime-id>/<episode>
// -> { "id": "...", "episode": 3, "position": 754.32 }
// (position is 0 when nothing is saved)
// ============================================================

fn handle_get_progress(request: Request, url: &str) {
    let remainder = match url.strip_prefix("/api/progress/") {
        Some(value) => value,
        None => {
            respond_text(request, StatusCode(404), "Not found");
            return;
        }
    };

    let mut parts = remainder.split('/');

    let anime_id_encoded = match parts.next() {
        Some(value) if !value.is_empty() => value,
        _ => {
            respond_text(request, StatusCode(400), "Missing anime id");
            return;
        }
    };

    let episode_text = match parts.next() {
        Some(value) if !value.is_empty() => value,
        _ => {
            respond_text(request, StatusCode(400), "Missing episode");
            return;
        }
    };

    if parts.next().is_some() {
        respond_text(request, StatusCode(400), "Invalid progress path");
        return;
    }

    let anime_id = match percent_decode(anime_id_encoded) {
        Ok(value) => value,
        Err(_) => {
            respond_text(request, StatusCode(400), "Invalid anime id");
            return;
        }
    };

    if contains_path_separator(&anime_id) || anime_id.is_empty() {
        respond_text(request, StatusCode(400), "Invalid anime id");
        return;
    }

    let episode: u32 = match episode_text.parse() {
        Ok(value) if value > 0 => value,
        _ => {
            respond_text(request, StatusCode(400), "Invalid episode");
            return;
        }
    };

    let position = match read_playback_position(&anime_id, episode) {
        Ok(position) => position,
        Err(error) => {
            eprintln!("Failed to read playback progress: {error}");
            respond_text(
                request,
                StatusCode(500),
                &format!("Failed to read playback progress: {error}"),
            );
            return;
        }
    };

    let body = json!({
        "id": anime_id,
        "episode": episode,
        "position": position
    })
    .to_string();

    respond_json(request, StatusCode(200), &body);
}

// ============================================================
// API: SAVE PLAYBACK PROGRESS
//
// POST /api/progress   { "id": "...", "episode": 3, "position": 754.32 }
// Stored as { "<id>": { "3": 754.32 } }
// ============================================================

fn handle_save_progress(mut request: Request) {
    let mut body = String::new();

    if let Err(error) = request.as_reader().read_to_string(&mut body) {
        respond_text(
            request,
            StatusCode(400),
            &format!("Failed to read request body: {error}"),
        );
        return;
    }

    let payload: ProgressRequest = match serde_json::from_str(&body) {
        Ok(payload) => payload,
        Err(error) => {
            respond_text(request, StatusCode(400), &format!("Invalid JSON: {error}"));
            return;
        }
    };

    if payload.id.trim().is_empty() || contains_path_separator(&payload.id) {
        respond_text(request, StatusCode(400), "Invalid anime id");
        return;
    }

    if payload.episode == 0 {
        respond_text(
            request,
            StatusCode(400),
            "Episode must be greater than zero",
        );
        return;
    }

    if !payload.position.is_finite() || payload.position < 0.0 {
        respond_text(request, StatusCode(400), "Invalid playback position");
        return;
    }

    match save_playback_position(&payload.id, payload.episode, payload.position) {
        Ok(()) => {
            let body = json!({
                "ok": true,
                "id": payload.id,
                "episode": payload.episode,
                "position": payload.position
            })
            .to_string();

            respond_json(request, StatusCode(200), &body);
        }
        Err(error) => respond_text(
            request,
            StatusCode(500),
            &format!("Failed to save playback progress: {error}"),
        ),
    }
}

// ============================================================
// PLAYBACK STORAGE
//
// Lives separately from catalog.json, ani-cli history, Orion
// history and prediction data.
// ============================================================

fn playback_lock() -> &'static Mutex<()> {
    PLAYBACK_LOCK.get_or_init(|| Mutex::new(()))
}

fn history_lock() -> &'static Mutex<()> {
    HISTORY_LOCK.get_or_init(|| Mutex::new(()))
}

fn playback_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("data")
        .join("playback.json")
}

fn read_playback_data() -> Result<Value, String> {
    let path = playback_path();

    if !path.exists() {
        return Ok(json!({}));
    }

    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;

    if content.trim().is_empty() {
        return Ok(json!({}));
    }

    serde_json::from_str(&content).map_err(|error| format!("Invalid playback.json: {error}"))
}

fn write_playback_data(data: &Value) -> Result<(), String> {
    let path = playback_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let output = serde_json::to_string_pretty(data).map_err(|error| error.to_string())?;

    let temporary_path = path.with_extension("json.tmp");

    fs::write(&temporary_path, format!("{output}\n")).map_err(|error| error.to_string())?;
    fs::rename(&temporary_path, &path).map_err(|error| error.to_string())?;

    Ok(())
}

fn read_playback_position(anime_id: &str, episode: u32) -> Result<f64, String> {
    let _guard = playback_lock()
        .lock()
        .map_err(|_| "Playback storage lock was poisoned".to_string())?;

    let data = read_playback_data()?;
    let episode_key = episode.to_string();

    let position = data
        .get(anime_id)
        .and_then(Value::as_object)
        .and_then(|episodes| episodes.get(&episode_key))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);

    Ok(position)
}

fn save_playback_position(anime_id: &str, episode: u32, position: f64) -> Result<(), String> {
    let _guard = playback_lock()
        .lock()
        .map_err(|_| "Playback storage lock was poisoned".to_string())?;

    let mut data = read_playback_data()?;

    let root = data
        .as_object_mut()
        .ok_or_else(|| "Playback storage root must be an object".to_string())?;

    let anime_entry = root
        .entry(anime_id.to_string())
        .or_insert_with(|| json!({}));

    let anime_object = anime_entry
        .as_object_mut()
        .ok_or_else(|| format!("Playback entry for '{anime_id}' must be an object"))?;

    anime_object.insert(episode.to_string(), Value::from(position));

    write_playback_data(&data)
}

// ============================================================
// MEDIA ROUTING
// ============================================================

fn handle_media(request: Request, is_video: bool) {
    // Owned, so `request` can be moved into the responders below.
    let full_url = request.url().to_string();
    let url = full_url.split('?').next().unwrap_or("").to_string();

    let prefix = if is_video { "/video/" } else { "/media/" };

    let remainder = match url.strip_prefix(prefix) {
        Some(value) => value,
        None => {
            respond_text(request, StatusCode(404), "Not found");
            return;
        }
    };

    let mut parts = remainder.splitn(2, '/');

    let anime_id = match parts.next() {
        Some(value) if !value.is_empty() => match percent_decode(value) {
            Ok(value) => value,
            Err(_) => {
                respond_text(request, StatusCode(400), "Invalid anime id");
                return;
            }
        },
        _ => {
            respond_text(request, StatusCode(400), "Missing anime id");
            return;
        }
    };

    let filename = match parts.next() {
        Some(value) if !value.is_empty() => match percent_decode(value) {
            Ok(value) => value,
            Err(_) => {
                respond_text(request, StatusCode(400), "Invalid filename");
                return;
            }
        },
        _ => {
            respond_text(request, StatusCode(400), "Missing filename");
            return;
        }
    };

    if contains_path_separator(&anime_id) || contains_path_separator(&filename) {
        respond_text(request, StatusCode(400), "Invalid path");
        return;
    }

    let file_path = media_root().join(&anime_id).join(&filename);

    if !file_path.is_file() {
        respond_text(request, StatusCode(404), "Media file not found");
        return;
    }

    if is_video {
        serve_video(request, file_path);
    } else {
        serve_static_file(request, file_path);
    }
}

// ============================================================
// SUBTITLES
//
// GET /subtitles/<anime-id>/<file>
// Serves .vtt directly and converts .srt to WebVTT on the fly, so the
// browser <track> element can use either.
// ============================================================

fn srt_to_vtt(srt: &str) -> String {
    let mut output = String::from("WEBVTT\n\n");

    for line in srt.trim_start_matches('\u{feff}').replace("\r\n", "\n").lines() {
        if line.contains("-->") {
            output.push_str(&line.replace(',', "."));
        } else {
            output.push_str(line);
        }

        output.push('\n');
    }

    output
}

fn handle_subtitles(request: Request) {
    let full_url = request.url().to_string();
    let url = full_url.split('?').next().unwrap_or("").to_string();

    let remainder = match url.strip_prefix("/subtitles/") {
        Some(value) => value.to_string(),
        None => {
            respond_text(request, StatusCode(404), "Not found");
            return;
        }
    };

    let mut parts = remainder.splitn(2, '/');

    let anime_id = match parts.next().filter(|value| !value.is_empty()).map(percent_decode) {
        Some(Ok(value)) => value,
        _ => {
            respond_text(request, StatusCode(400), "Invalid anime id");
            return;
        }
    };

    let filename = match parts.next().filter(|value| !value.is_empty()).map(percent_decode) {
        Some(Ok(value)) => value,
        _ => {
            respond_text(request, StatusCode(400), "Invalid filename");
            return;
        }
    };

    if contains_path_separator(&anime_id) || contains_path_separator(&filename) {
        respond_text(request, StatusCode(400), "Invalid path");
        return;
    }

    let requested = media_root().join(&anime_id).join(&filename);

    // Exact file first, then the same name with the other subtitle extension
    // (the player asks for "<video name>.vtt" when no subtitle is listed).
    let candidates = [
        requested.clone(),
        requested.with_extension("srt"),
        requested.with_extension("vtt"),
    ];

    let Some(found) = candidates.into_iter().find(|path| path.is_file()) else {
        respond_text(request, StatusCode(404), "Subtitle not found");
        return;
    };

    match file_extension(&found).as_str() {
        "srt" => match fs::read(&found) {
            Ok(bytes) => {
                let vtt = srt_to_vtt(&String::from_utf8_lossy(&bytes));

                let response = Response::from_string(vtt)
                    .with_status_code(StatusCode(200))
                    .with_header(header("Content-Type", "text/vtt; charset=utf-8"))
                    .with_header(header("Access-Control-Allow-Origin", "*"));

                let _ = request.respond(response);
            }
            Err(error) => respond_text(
                request,
                StatusCode(500),
                &format!("Failed to read subtitle: {error}"),
            ),
        },
        "vtt" => serve_static_file(request, found),
        _ => respond_text(request, StatusCode(404), "Unsupported subtitle format"),
    }
}

// ============================================================
// VIDEO
// ============================================================

fn serve_video(request: Request, path: PathBuf) {
    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(error) => {
            respond_text(
                request,
                StatusCode(500),
                &format!("Failed to open video: {error}"),
            );
            return;
        }
    };

    let size = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            respond_text(
                request,
                StatusCode(500),
                &format!("Failed to read video metadata: {error}"),
            );
            return;
        }
    };

    if size == 0 {
        let response = Response::new(
            StatusCode(200),
            vec![
                header("Content-Type", mime_type(&path)),
                header("Content-Length", "0"),
                header("Accept-Ranges", "bytes"),
                header("Access-Control-Allow-Origin", "*"),
            ],
            file,
            Some(0),
            None,
        );

        let _ = request.respond(response);
        return;
    }

    let range = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Range"))
        .and_then(|header| parse_range(header.value.as_str(), size));

    let (start, end, status) = match range {
        Some((start, end)) => (start, end, StatusCode(206)),
        None => (0, size - 1, StatusCode(200)),
    };

    if let Err(error) = file.seek(SeekFrom::Start(start)) {
        respond_text(
            request,
            StatusCode(500),
            &format!("Failed to seek video: {error}"),
        );
        return;
    }

    let content_length = end - start + 1;
    let limited_file = file.take(content_length);

    let content_length_usize = match usize::try_from(content_length) {
        Ok(value) => value,
        Err(_) => {
            respond_text(
                request,
                StatusCode(500),
                "Video chunk is too large for this platform",
            );
            return;
        }
    };

    let mut headers = vec![
        header("Content-Type", mime_type(&path)),
        header("Accept-Ranges", "bytes"),
        header("Content-Length", &content_length.to_string()),
        header("Access-Control-Allow-Origin", "*"),
    ];

    if status == StatusCode(206) {
        headers.push(header(
            "Content-Range",
            &format!("bytes {start}-{end}/{size}"),
        ));
    }

    let response = Response::new(
        status,
        headers,
        limited_file,
        Some(content_length_usize),
        None,
    );

    // The browser often aborts a range mid-stream when it seeks; that
    // just makes this write fail, which is fine.
    let _ = request.respond(response);
}

// ============================================================
// STATIC MEDIA
// ============================================================

fn serve_static_file(request: Request, path: PathBuf) {
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) => {
            respond_text(
                request,
                StatusCode(500),
                &format!("Failed to open media file: {error}"),
            );
            return;
        }
    };

    let size = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            respond_text(
                request,
                StatusCode(500),
                &format!("Failed to read file metadata: {error}"),
            );
            return;
        }
    };

    let size_usize = match usize::try_from(size) {
        Ok(value) => value,
        Err(_) => {
            respond_text(request, StatusCode(500), "File is too large for this platform");
            return;
        }
    };

    let response = Response::new(
        StatusCode(200),
        vec![
            header("Content-Type", mime_type(&path)),
            header("Content-Length", &size.to_string()),
            header("Access-Control-Allow-Origin", "*"),
        ],
        file,
        Some(size_usize),
        None,
    );

    let _ = request.respond(response);
}

// ============================================================
// LIKE
//
// A rating must NOT depend on ani-cli history. The catalog is the
// source of truth; ani-cli history is only updated if an existing
// entry can actually be found.
// ============================================================

fn like_history(payload: &HistoryRequest) -> Result<(), String> {
    // Orion history: "_" = liked
    upsert_orion_history(payload.episode, "_", &payload.title)?;

    // Ratings are separate from ani-cli watch history and prediction inputs.
    update_catalog_rating(&payload.id, "liked", None)?;

    Ok(())
}

// ============================================================
// DISLIKE
// ============================================================

fn dislike_history(payload: &HistoryRequest) -> Result<(), String> {
    // Remove from Orion history.
    remove_orion_history(&payload.title)?;

    update_catalog_rating(&payload.id, "disliked", None)?;

    Ok(())
}

// ============================================================
// ORION HISTORY
//
// Format:  episode<TAB>marker<TAB>Anime Title
// "_" = liked, "-" = neutral. Disliked anime are removed entirely.
// ============================================================

fn orion_history_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("state")
        .join("orion-history")
        .join("history")
}

fn upsert_orion_history(episode: u32, marker: &str, title: &str) -> Result<(), String> {
    if marker != "_" && marker != "-" {
        return Err(format!("Invalid Orion history marker: '{marker}'"));
    }

    let path = orion_history_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let content = if path.exists() {
        fs::read_to_string(&path).map_err(|error| error.to_string())?
    } else {
        String::new()
    };

    let wanted_title = normalize_title(title);
    let new_line = format!("{}\t{}\t{}", episode, marker, title);

    let mut output = String::new();
    let mut replaced = false;

    for line in content.lines() {
        let fields: Vec<&str> = line.splitn(3, '\t').collect();

        if fields.len() == 3 {
            let existing_title = fields[2];

            if normalize_title(existing_title) == wanted_title {
                if !replaced {
                    output.push_str(&new_line);
                    output.push('\n');
                    replaced = true;
                }

                continue;
            }
        }

        if !line.is_empty() {
            output.push_str(line);
            output.push('\n');
        }
    }

    if !replaced {
        output.push_str(&new_line);
        output.push('\n');
    }

    fs::write(path, output).map_err(|error| error.to_string())
}

fn remove_orion_history(title: &str) -> Result<(), String> {
    let path = orion_history_path();

    if !path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;

    let wanted_title = normalize_title(title);

    let mut output = String::new();

    for line in content.lines() {
        let fields: Vec<&str> = line.splitn(3, '\t').collect();

        let matches_title = fields.len() == 3 && normalize_title(fields[2]) == wanted_title;

        if matches_title {
            continue;
        }

        if !line.is_empty() {
            output.push_str(line);
            output.push('\n');
        }
    }

    fs::write(path, output).map_err(|error| error.to_string())
}

// ============================================================
// CATALOG HISTORY ID
// ============================================================

fn catalog_history_id(anime_id: &str) -> Result<Option<String>, String> {
    let content = fs::read_to_string(catalog_path()).map_err(|error| error.to_string())?;

    let catalog: Value = serde_json::from_str(&content).map_err(|error| error.to_string())?;

    let history_id = catalog
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("id").and_then(Value::as_str) == Some(anime_id))
        })
        .and_then(|item| item.get("historyId"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    Ok(history_id)
}

// ============================================================
// CATALOG RATING
// ============================================================

fn update_catalog_rating(
    anime_id: &str,
    rating: &str,
    history_id: Option<&str>,
) -> Result<(), String> {
    if rating != "liked" && rating != "disliked" && rating != "unrated" {
        return Err(format!("Invalid rating: '{rating}'"));
    }

    let path = catalog_path();

    let content = fs::read_to_string(&path).map_err(|error| error.to_string())?;

    let mut catalog: Value = serde_json::from_str(&content).map_err(|error| error.to_string())?;

    let items = catalog
        .as_array_mut()
        .ok_or_else(|| "Catalog root must be an array".to_string())?;

    let item = items
        .iter_mut()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(anime_id))
        .ok_or_else(|| format!("Anime '{anime_id}' not found in catalog"))?;

    item["rating"] = Value::String(rating.to_string());

    // Keep historyId when we actually have one.
    if let Some(history_id) = history_id {
        item["historyId"] = Value::String(history_id.to_string());
    }

    // A disliked anime should not retain a stale historyId.
    if rating == "disliked" && history_id.is_none() {
        if let Some(object) = item.as_object_mut() {
            object.remove("historyId");
        }
    }

    let output = serde_json::to_string_pretty(&catalog).map_err(|error| error.to_string())?;

    // Write to a temp file and rename so a crash or a concurrent read
    // never sees a half-written catalog.
    let temporary = path.with_extension("json.tmp");

    fs::write(&temporary, format!("{output}\n")).map_err(|error| error.to_string())?;
    fs::rename(&temporary, &path).map_err(|error| error.to_string())?;

    Ok(())
}

// ============================================================
// ANI-CLI HISTORY
// ============================================================

fn find_history_line(
    path: &PathBuf,
    anime_id: &str,
    title: &str,
) -> Result<Option<String>, String> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;

    let wanted_title = normalize_title(title);

    for line in content.lines() {
        let fields: Vec<&str> = line.splitn(3, '\t').collect();

        if fields.len() != 3 {
            continue;
        }

        let line_id = fields[1];
        let line_title = fields[2];

        if line_id == anime_id || normalize_title(line_title) == wanted_title {
            return Ok(Some(line.to_string()));
        }
    }

    Ok(None)
}

fn upsert_history_line(path: &PathBuf, new_line: &str, history_id: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let content = if path.exists() {
        fs::read_to_string(path).map_err(|error| error.to_string())?
    } else {
        String::new()
    };

    let mut output = String::new();
    let mut replaced = false;

    for line in content.lines() {
        let fields: Vec<&str> = line.splitn(3, '\t').collect();

        if fields.len() == 3 && fields[1] == history_id {
            if !replaced {
                output.push_str(new_line);
                output.push('\n');
                replaced = true;
            }
        } else if !line.is_empty() {
            output.push_str(line);
            output.push('\n');
        }
    }

    if !replaced {
        output.push_str(new_line);
        output.push('\n');
    }

    fs::write(path, output).map_err(|error| error.to_string())
}

fn remove_history_entry(
    path: &PathBuf,
    history_id: Option<&str>,
    title: Option<&str>,
) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;

    let wanted_title = title.map(normalize_title);

    let mut output = String::new();

    for line in content.lines() {
        let fields: Vec<&str> = line.splitn(3, '\t').collect();

        let matches_id = history_id.is_some() && fields.get(1).copied() == history_id;

        let matches_title = match (wanted_title.as_deref(), fields.get(2)) {
            (Some(wanted), Some(actual)) => normalize_title(actual) == wanted,
            _ => false,
        };

        if matches_id || matches_title {
            continue;
        }

        if !line.is_empty() {
            output.push_str(line);
            output.push('\n');
        }
    }

    fs::write(path, output).map_err(|error| error.to_string())
}

// ============================================================
// PATHS
// ============================================================

fn media_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("media")
}

fn catalog_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("data")
        .join("catalog.json")
}

fn private_history_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("state")
        .join("ani-cli-download-history")
        .join("ani-hsts")
}

fn public_history_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| "HOME environment variable not found".to_string())?;

    let state_dir = match std::env::var_os("XDG_STATE_HOME") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(home).join(".local").join("state"),
    };

    Ok(state_dir.join("ani-cli").join("ani-hsts"))
}

// ============================================================
// LIBRARY COUNT
// ============================================================

fn count_anime_directories() -> io::Result<usize> {
    let root = media_root();

    if !root.exists() {
        return Ok(0);
    }

    let mut count = 0;

    for entry in fs::read_dir(root)? {
        let entry = entry?;

        if entry.path().is_dir() {
            count += 1;
        }
    }

    Ok(count)
}

// ============================================================
// HELPERS
// ============================================================

fn normalize_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn respond_json(request: Request, status: StatusCode, body: &str) {
    let response = Response::from_string(body)
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
        .with_header(header("Access-Control-Allow-Origin", "*"))
        .with_header(header("Access-Control-Allow-Headers", "Content-Type"))
        .with_header(header("Access-Control-Allow-Methods", "GET, POST, OPTIONS"));

    let _ = request.respond(response);
}

fn respond_text(request: Request, status: StatusCode, body: &str) {
    let response = Response::from_string(body)
        .with_status_code(status)
        .with_header(header("Content-Type", "text/plain; charset=utf-8"))
        .with_header(header("Access-Control-Allow-Origin", "*"))
        .with_header(header("Access-Control-Allow-Headers", "Content-Type"))
        .with_header(header("Access-Control-Allow-Methods", "GET, POST, OPTIONS"));

    let _ = request.respond(response);
}

fn respond_empty(request: Request, status: StatusCode) {
    let response = Response::empty(status)
        .with_header(header("Access-Control-Allow-Origin", "*"))
        .with_header(header("Access-Control-Allow-Headers", "Content-Type"))
        .with_header(header("Access-Control-Allow-Methods", "GET, POST, OPTIONS"));

    let _ = request.respond(response);
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid HTTP header")
}

fn mime_type(path: &PathBuf) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mkv") => "video/x-matroska",
        Some("mov") => "video/quicktime",
        Some("avi") => "video/x-msvideo",
        Some("vtt") => "text/vtt; charset=utf-8",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}

fn parse_range(value: &str, size: u64) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes=")?;

    if value.contains(',') {
        return None;
    }

    let mut parts = value.splitn(2, '-');

    let start = parts.next()?.trim();
    let end = parts.next()?.trim();

    // bytes=-500
    if start.is_empty() {
        let suffix_len = end.parse::<u64>().ok()?;

        if suffix_len == 0 {
            return None;
        }

        let length = suffix_len.min(size);

        return Some((size - length, size - 1));
    }

    let start = start.parse::<u64>().ok()?;

    if start >= size {
        return None;
    }

    let end = if end.is_empty() {
        size - 1
    } else {
        end.parse::<u64>().ok()?.min(size - 1)
    };

    if start > end {
        None
    } else {
        Some((start, end))
    }
}

fn percent_decode(value: &str) -> Result<String, ()> {
    let bytes = value.as_bytes();

    let mut output = Vec::with_capacity(bytes.len());

    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(());
            }

            let high = hex_value(bytes[index + 1]).ok_or(())?;
            let low = hex_value(bytes[index + 2]).ok_or(())?;

            output.push((high << 4) | low);

            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(output).map_err(|_| ())
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn contains_path_separator(value: &str) -> bool {
    value.contains('/') || value.contains('\\') || value == "." || value == ".."
}