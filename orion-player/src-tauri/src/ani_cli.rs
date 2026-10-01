use std::{
    collections::{HashMap, HashSet},
    env,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};

use crate::prediction;
use serde_json::{json, Value};

const LIBRARY_LIMIT: usize = 40;
const NEW_ANIME_PER_GENERATION: usize = 5;
const ANILIST_API_URL: &str = "https://graphql.anilist.co";

static CATALOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static GENERATION_RUNNING: OnceLock<Mutex<bool>> = OnceLock::new();

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct AnimeMetadata {
    title: String,
    english_title: Option<String>,
    romaji_title: Option<String>,
    genres: Vec<String>,
    episodes: Option<u32>,
    poster: Option<String>,
    format: Option<String>,
}

// ============================================================
// LOCKS
// ============================================================

fn catalog_lock() -> &'static Mutex<()> {
    CATALOG_LOCK.get_or_init(|| Mutex::new(()))
}

fn generation_lock() -> &'static Mutex<bool> {
    GENERATION_RUNNING.get_or_init(|| Mutex::new(false))
}

struct GenerationGuard;

impl Drop for GenerationGuard {
    fn drop(&mut self) {
        if let Ok(mut running) = generation_lock().lock() {
            *running = false;
        }
    }
}

fn try_begin_generation() -> Option<GenerationGuard> {
    let mut running = generation_lock().lock().ok()?;

    if *running {
        return None;
    }

    *running = true;
    Some(GenerationGuard)
}

// ============================================================
// PATHS
// ============================================================

fn project_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    manifest
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}

fn media_directory() -> PathBuf {
    project_root().join("src-tauri").join("media")
}

fn public_media_directory() -> PathBuf {
    project_root().join("public").join("media")
}

fn catalog_path() -> PathBuf {
    project_root().join("data").join("catalog.json")
}

fn orion_history_path() -> PathBuf {
    project_root()
        .join("src-tauri")
        .join("state")
        .join("orion-history")
        .join("history")
}

fn private_ani_cli_history_path() -> PathBuf {
    project_root()
        .join("src-tauri")
        .join("state")
        .join("ani-cli-download-history")
        .join("ani-hsts")
}

fn public_ani_cli_history_path() -> PathBuf {
    if let Ok(directory) = env::var("ANI_CLI_HIST_DIR") {
        return PathBuf::from(directory).join("ani-hsts");
    }

    let state_home = env::var("XDG_STATE_HOME").unwrap_or_else(|_| {
        let home = env::var("HOME").unwrap_or_default();
        format!("{home}/.local/state")
    });

    PathBuf::from(state_home)
        .join("ani-cli")
        .join("ani-hsts")
}

// ============================================================
// TITLE / FOLDER NORMALIZATION
// ============================================================

fn normalize_title(title: &str) -> String {
    title
        .chars()
        .flat_map(|character| character.to_lowercase())
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn titles_match(left: &str, right: &str) -> bool {
    normalize_title(left) == normalize_title(right)
}

fn fnv1a_32(value: &str) -> u32 {
    let mut hash = 0x811c9dc5u32;

    for byte in value.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x01000193);
    }

    hash
}

fn anime_folder_name(title: &str) -> String {
    let normalized = normalize_title(title);

    if normalized.is_empty() {
        return format!("anime-{:08x}", fnv1a_32(title));
    }

    normalized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

fn anime_directory(title: &str) -> PathBuf {
    media_directory().join(anime_folder_name(title))
}

fn poster_directory(title: &str) -> PathBuf {
    public_media_directory().join(anime_folder_name(title))
}

// ============================================================
// CATALOG
// ============================================================

fn read_catalog_unlocked() -> Vec<Value> {
    let path = catalog_path();

    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    match serde_json::from_str::<Value>(&contents) {
        Ok(Value::Array(entries)) => entries,

        Ok(_) => Vec::new(),

        Err(error) => {
            eprintln!("⚠️ Failed to parse catalog.json: {error}");
            Vec::new()
        }
    }
}

fn write_catalog_unlocked(entries: &[Value]) -> bool {
    let path = catalog_path();

    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            eprintln!("❌ Failed to create catalog directory: {error}");
            return false;
        }
    }

    let temporary = path.with_extension("json.tmp");
    let backup = path.with_extension("json.bak");

    let serialized = match serde_json::to_string_pretty(entries) {
        Ok(value) => value,

        Err(error) => {
            eprintln!("❌ Failed to serialize catalog: {error}");
            return false;
        }
    };

    if path.exists() {
        let _ = fs::copy(&path, &backup);
    }

    if let Err(error) = fs::write(&temporary, serialized) {
        eprintln!("❌ Failed to write temporary catalog: {error}");
        return false;
    }

    if let Err(error) = fs::rename(&temporary, &path) {
        eprintln!("❌ Failed to replace catalog: {error}");
        let _ = fs::remove_file(&temporary);
        return false;
    }

    true
}

fn read_catalog() -> Vec<Value> {
    let _guard = catalog_lock().lock().ok();
    read_catalog_unlocked()
}

#[allow(dead_code)]
fn write_catalog(entries: &[Value]) -> bool {
    let _guard = catalog_lock().lock().ok();
    write_catalog_unlocked(entries)
}

fn catalog_contains_title(catalog: &[Value], title: &str) -> bool {
    catalog.iter().any(|entry| {
        entry
            .get("title")
            .and_then(Value::as_str)
            .map(|existing| titles_match(existing, title))
            .unwrap_or(false)
    })
}

#[allow(dead_code)]
fn catalog_rating(catalog: &[Value], title: &str) -> Option<String> {
    catalog
        .iter()
        .find(|entry| {
            entry
                .get("title")
                .and_then(Value::as_str)
                .map(|existing| titles_match(existing, title))
                .unwrap_or(false)
        })
        .and_then(|entry| {
            entry
                .get("rating")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

// ============================================================
// ORION HISTORY
// ============================================================

#[derive(Clone, Debug)]
struct OrionHistoryEntry {
    episode: u32,
    marker: char,
    title: String,
}

fn read_orion_history() -> Vec<OrionHistoryEntry> {
    let path = orion_history_path();

    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    contents
        .lines()
        .filter_map(parse_orion_history_line)
        .collect()
}

fn parse_orion_history_line(line: &str) -> Option<OrionHistoryEntry> {
    let mut parts = line.splitn(3, '\t');

    let episode = parts.next()?.trim().parse::<u32>().ok()?;

    let marker = parts.next()?.chars().next()?;

    let title = parts.next()?.trim();

    if title.is_empty() {
        return None;
    }

    if marker != '_' && marker != '-' {
        return None;
    }

    Some(OrionHistoryEntry {
        episode,
        marker,
        title: title.to_string(),
    })
}

fn liked_history_entries() -> Vec<String> {
    let mut titles = Vec::new();
    let mut seen = HashSet::new();

    for entry in read_orion_history() {
        if entry.marker != '_' {
            continue;
        }

        let key = normalize_title(&entry.title);

        if seen.insert(key) {
            titles.push(entry.title);
        }
    }

    titles
}

fn liked_history_contains(title: &str) -> bool {
    read_orion_history().iter().any(|entry| {
        entry.marker == '_' && titles_match(&entry.title, title)
    })
}

// ============================================================
// ANI-CLI HISTORY
// ============================================================

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct AniCliHistoryEntry {
    episode: u32,
    history_id: String,
    title: String,
}

fn parse_history_line(line: &str) -> Option<AniCliHistoryEntry> {
    let mut parts = line.splitn(3, '\t');

    let episode = parts.next()?.trim().parse::<u32>().ok()?;

    let history_id = parts.next()?.trim().to_string();

    let title = parts.next()?.trim().to_string();

    if title.is_empty() {
        return None;
    }

    Some(AniCliHistoryEntry {
        episode,
        history_id,
        title,
    })
}

fn read_history_file(path: &Path) -> Vec<AniCliHistoryEntry> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    contents
        .lines()
        .filter_map(parse_history_line)
        .collect()
}

fn find_history_entry_in(
    path: &Path,
    title: &str,
) -> Option<AniCliHistoryEntry> {
    read_history_file(path)
        .into_iter()
        .rev()
        .find(|entry| titles_match(&entry.title, title))
}

fn find_history_entry(title: &str) -> Option<AniCliHistoryEntry> {
    find_history_entry_in(&private_ani_cli_history_path(), title)
        .or_else(|| {
            find_history_entry_in(
                &public_ani_cli_history_path(),
                title,
            )
        })
}

fn find_any_history_entry(title: &str) -> Option<AniCliHistoryEntry> {
    find_history_entry(title)
}

#[allow(dead_code)]
fn read_private_history() -> Vec<AniCliHistoryEntry> {
    read_history_file(&private_ani_cli_history_path())
}

#[allow(dead_code)]
fn read_public_history() -> Vec<AniCliHistoryEntry> {
    read_history_file(&public_ani_cli_history_path())
}

// ============================================================
// EPISODE DETECTION
// ============================================================

fn digits_after_marker(
    name: &str,
    marker: char,
) -> Option<u32> {
    let position = name.rfind(marker)?;

    let after = &name[position + marker.len_utf8()..];

    let digits: String = after
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();

    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn episode_number_from_filename(path: &Path) -> Option<u32> {
    let stem = path.file_stem()?.to_string_lossy();

    // Examples:
    //
    // Anime Episode 1
    // Anime Episode 12
    // Anime - E13
    // Anime E24

    for marker in ['e', 'E'] {
        if let Some(number) =
            digits_after_marker(&stem, marker)
        {
            return Some(number);
        }
    }

    // Fallback:
    //
    // Anime 01
    // Anime 12
    // Anime 120

    let mut number = String::new();

    for character in stem.chars().rev() {
        if character.is_ascii_digit() {
            number.insert(0, character);
        } else if !number.is_empty() {
            break;
        }
    }

    number.parse().ok()
}

fn is_video_extension(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    matches!(
        extension.as_str(),
        "mp4" | "mkv" | "webm" | "m4v" | "mov" | "avi"
    )
}

fn is_subtitle_extension(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    matches!(
        extension.as_str(),
        "srt" | "vtt" | "ass" | "ssa"
    )
}

fn is_temporary_video(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            name.contains(".optimized.")
                || name.contains(".part.")
                || name.ends_with(".tmp")
        })
        .unwrap_or(false)
}

fn existing_episode_numbers(title: &str) -> HashSet<u32> {
    let directory = anime_directory(title);
    let mut episodes = HashSet::new();

    let Ok(entries) = fs::read_dir(directory) else {
        return episodes;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if !path.is_file()
            || !is_video_extension(&path)
            || is_temporary_video(&path)
        {
            continue;
        }

        if let Some(episode) =
            episode_number_from_filename(&path)
        {
            episodes.insert(episode);
        }
    }

    episodes
}

fn next_episode_number(title: &str) -> u32 {
    let episodes = existing_episode_numbers(title);

    if episodes.is_empty() {
        return 1;
    }

    let mut episode = 1;

    loop {
        if !episodes.contains(&episode) {
            return episode;
        }

        episode += 1;
    }
}

fn find_episode_file(
    title: &str,
    episode: u32,
) -> Option<PathBuf> {
    let directory = anime_directory(title);

    let Ok(entries) = fs::read_dir(directory) else {
        return None;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if !path.is_file()
            || !is_video_extension(&path)
            || is_temporary_video(&path)
        {
            continue;
        }

        if episode_number_from_filename(&path)
            == Some(episode)
        {
            return Some(path);
        }
    }

    None
}

fn find_episode_video(
    title: &str,
    episode: u32,
) -> Option<PathBuf> {
    find_episode_file(title, episode)
}

fn find_episode_subtitle(
    title: &str,
    episode: u32,
) -> Option<PathBuf> {
    let directory = anime_directory(title);

    let Ok(entries) = fs::read_dir(directory) else {
        return None;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if !path.is_file()
            || !is_subtitle_extension(&path)
        {
            continue;
        }

        if episode_number_from_filename(&path)
            == Some(episode)
        {
            return Some(path);
        }
    }

    None
}

// ============================================================
// ANILIST
// ============================================================

fn anilist_request(
    query: &str,
    variables: Value,
) -> Option<Value> {
    let payload = json!({
        "query": query,
        "variables": variables,
    });

    let body =
        serde_json::to_string(&payload).ok()?;

    let response = ureq::post(ANILIST_API_URL)
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .set("User-Agent", "Orion/0.1")
        .send_string(&body)
        .ok()?;

    let text = response.into_string().ok()?;

    serde_json::from_str::<Value>(&text).ok()
}

fn fetch_anilist_metadata(
    title: &str,
) -> Option<AnimeMetadata> {
    let query = r#"
        query ($search: String) {
            Media(search: $search, type: ANIME) {
                title {
                    romaji
                    english
                    native
                }

                genres
                episodes
                format

                coverImage {
                    large
                    extraLarge
                }
            }
        }
    "#;

    let response = anilist_request(
        query,
        json!({
            "search": title
        }),
    )?;

    let media = response
        .get("data")
        .and_then(|value| value.get("Media"))?;

    let title_object = media.get("title")?;

    let romaji_title = title_object
        .get("romaji")
        .and_then(Value::as_str)
        .map(str::to_string);

    let english_title = title_object
        .get("english")
        .and_then(Value::as_str)
        .map(str::to_string);

    let resolved_title = english_title
        .clone()
        .or_else(|| romaji_title.clone())
        .unwrap_or_else(|| title.to_string());

    let genres = media
        .get("genres")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let episodes = media
        .get("episodes")
        .and_then(Value::as_u64)
        .map(|value| value as u32);

    let format = media
        .get("format")
        .and_then(Value::as_str)
        .map(str::to_string);

    let poster = media
        .get("coverImage")
        .and_then(|value| {
            value
                .get("extraLarge")
                .or_else(|| value.get("large"))
        })
        .and_then(Value::as_str)
        .map(str::to_string);

    Some(AnimeMetadata {
        title: resolved_title,
        english_title,
        romaji_title,
        genres,
        episodes,
        poster,
        format,
    })
}

// ============================================================
// POSTERS
// ============================================================

fn fetch_poster_bytes(url: &str) -> Option<Vec<u8>> {
    println!("🌐 Downloading poster from:");
    println!("   {url}");

    let response = match ureq::get(url)
        .set("User-Agent", "Orion/0.1")
        .set(
            "Accept",
            "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8",
        )
        .call()
    {
        Ok(response) => response,

        Err(error) => {
            eprintln!("❌ Poster HTTP request failed: {error}");
            eprintln!("   URL: {url}");
            return None;
        }
    };

    let mut reader = response.into_reader();
    let mut bytes = Vec::new();

    if let Err(error) =
        reader.read_to_end(&mut bytes)
    {
        eprintln!(
            "❌ Failed to read poster response: {error}"
        );
        return None;
    }

    if bytes.is_empty() {
        eprintln!("❌ Poster response was empty");
        return None;
    }

    println!(
        "✅ Poster downloaded: {} bytes",
        bytes.len()
    );

    Some(bytes)
}

fn download_thumbnail(
    title: &str,
    poster_url: &str,
) -> bool {
    let title = title.trim();

    if title.is_empty()
        || poster_url.trim().is_empty()
    {
        eprintln!(
            "❌ Cannot download poster: missing title or URL"
        );
        return false;
    }

    let Some(bytes) =
        fetch_poster_bytes(poster_url)
    else {
        eprintln!(
            "❌ Failed to download poster for {title}"
        );
        return false;
    };

    // --------------------------------------------------------
    // LOCAL MEDIA POSTER
    // --------------------------------------------------------

    let anime_media_directory =
        anime_directory(title);

    if let Err(error) =
        fs::create_dir_all(&anime_media_directory)
    {
        eprintln!(
            "❌ Failed to create anime media directory {}: {error}",
            anime_media_directory.display()
        );

        return false;
    }

    let local_poster_path =
        anime_media_directory.join("poster.jpg");

    if let Err(error) =
        fs::write(&local_poster_path, &bytes)
    {
        eprintln!(
            "❌ Failed to save local poster {}: {error}",
            local_poster_path.display()
        );

        return false;
    }

    println!(
        "🖼️ Poster saved to {}",
        local_poster_path.display()
    );

    // --------------------------------------------------------
    // PUBLIC FRONTEND POSTER
    // --------------------------------------------------------

    let public_directory =
        poster_directory(title);

    if let Err(error) =
        fs::create_dir_all(&public_directory)
    {
        eprintln!(
            "❌ Failed to create public poster directory {}: {error}",
            public_directory.display()
        );

        return false;
    }

    let public_poster_path =
        public_directory.join("poster.jpg");

    if let Err(error) =
        fs::write(&public_poster_path, &bytes)
    {
        eprintln!(
            "❌ Failed to save public poster {}: {error}",
            public_poster_path.display()
        );

        return false;
    }

    println!(
        "🖼️ Poster saved to {}",
        public_poster_path.display()
    );

    println!(
        "✅ Poster installed successfully for {title}"
    );

    true
}

// ============================================================
// CATALOG EPISODE BUILDER
// ============================================================

fn build_catalog_episodes(
    title: &str,
) -> Vec<Value> {
    let directory = anime_directory(title);

    let Ok(entries) =
        fs::read_dir(&directory)
    else {
        return Vec::new();
    };

    let mut discovered:
        HashMap<u32, (PathBuf, Option<PathBuf>)> =
        HashMap::new();

    for entry in entries.flatten() {
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        if is_temporary_video(&path) {
            continue;
        }

        if is_video_extension(&path) {
            let Some(number) =
                episode_number_from_filename(&path)
            else {
                eprintln!(
                    "⚠️ Could not determine episode number from {}",
                    path.display()
                );

                continue;
            };

            let subtitle =
                find_episode_subtitle(title, number);

            discovered.insert(
                number,
                (path, subtitle),
            );
        }
    }

    let mut episode_numbers =
        discovered.keys().copied().collect::<Vec<_>>();

    episode_numbers.sort_unstable();

    let mut episodes = Vec::new();

    for number in episode_numbers {
        let Some((video_path, subtitle_path)) =
            discovered.remove(&number)
        else {
            continue;
        };

        let Some(video_filename) =
            video_path
                .file_name()
                .and_then(|name| name.to_str())
        else {
            continue;
        };

        let mut episode = json!({
            "number": number,
            "video": video_filename
        });

        if let Some(subtitle_path) =
            subtitle_path
        {
            if let Some(subtitle_filename) =
                subtitle_path
                    .file_name()
                    .and_then(|name| name.to_str())
            {
                episode["subtitle"] =
                    json!(subtitle_filename);
            }
        }

        episodes.push(episode);
    }

    episodes
}

// ============================================================
// CATALOG UPDATE
// ============================================================

fn update_catalog(
    title: &str,
    metadata: Option<&AnimeMetadata>,
) -> bool {
    let _guard = match catalog_lock().lock() {
        Ok(guard) => guard,

        Err(error) => {
            eprintln!(
                "❌ Catalog lock poisoned: {error}"
            );

            return false;
        }
    };

    let mut catalog =
        read_catalog_unlocked();

    let folder =
        anime_folder_name(title);

    let existing_index =
        catalog.iter().position(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .map(|id| id == folder)
                .unwrap_or(false)
                ||
            entry
                .get("title")
                .and_then(Value::as_str)
                .map(|existing| {
                    titles_match(existing, title)
                })
                .unwrap_or(false)
        });

    let mut entry = existing_index
        .and_then(|index| {
            catalog.get(index).cloned()
        })
        .unwrap_or_else(|| {
            json!({
                "id": folder,
                "title": title,
                "genres": [],
                "totalEpisodes": null,
                "format": "TV",
                "poster": "poster.jpg",
                "episodes": []
            })
        });

    if !entry.is_object() {
        entry = json!({});
    }

    let object = entry
        .as_object_mut()
        .expect("catalog entry must be an object");

    // --------------------------------------------------------
    // BASIC INFORMATION
    // --------------------------------------------------------

    object.insert(
        "id".to_string(),
        json!(folder),
    );

    object.insert(
        "title".to_string(),
        json!(title),
    );

    object.insert(
        "poster".to_string(),
        json!("poster.jpg"),
    );

    // --------------------------------------------------------
    // METADATA
    // --------------------------------------------------------

    if let Some(metadata) = metadata {
        object.insert(
            "genres".to_string(),
            json!(metadata.genres),
        );

        object.insert(
            "totalEpisodes".to_string(),
            metadata
                .episodes
                .map(|value| json!(value))
                .unwrap_or(Value::Null),
        );

        object.insert(
            "format".to_string(),
            metadata
                .format
                .clone()
                .map(Value::String)
                .unwrap_or_else(|| json!("TV")),
        );

        if let Some(english) =
            &metadata.english_title
        {
            object.insert(
                "englishTitle".to_string(),
                json!(english),
            );
        }

        if let Some(romaji) =
            &metadata.romaji_title
        {
            object.insert(
                "romajiTitle".to_string(),
                json!(romaji),
            );
        }
    } else {
        object
            .entry("genres".to_string())
            .or_insert_with(|| json!([]));

        object
            .entry("totalEpisodes".to_string())
            .or_insert(Value::Null);

        object
            .entry("format".to_string())
            .or_insert_with(|| json!("TV"));
    }

    // --------------------------------------------------------
    // REAL EPISODES FROM DISK
    // --------------------------------------------------------

    let episodes =
        build_catalog_episodes(title);

    object.insert(
        "episodes".to_string(),
        Value::Array(episodes),
    );

    // --------------------------------------------------------
    // HISTORY ID
    // --------------------------------------------------------

    if let Some(history) =
        find_any_history_entry(title)
    {
        object.insert(
            "historyId".to_string(),
            json!(history.history_id),
        );
    }

    // --------------------------------------------------------
    // RATING
    // --------------------------------------------------------

    // Never overwrite an existing rating.

    object
        .entry("rating".to_string())
        .or_insert_with(|| json!("not rated"));

    // --------------------------------------------------------
    // SAVE
    // --------------------------------------------------------

    match existing_index {
        Some(index) => {
            catalog[index] = entry;
        }

        None => {
            catalog.push(entry);
        }
    }

    write_catalog_unlocked(&catalog)
}

// ============================================================
// FFMPEG
// ============================================================

fn probe_video_stream(
    path: &Path,
) -> Option<(String, String)> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,pix_fmt",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text =
        String::from_utf8_lossy(&output.stdout);

    let mut codec = String::new();
    let mut pix_fmt = String::new();

    for line in text.lines() {
        if let Some(value) =
            line.strip_prefix("codec_name=")
        {
            codec = value.trim().to_string();
        } else if let Some(value) =
            line.strip_prefix("pix_fmt=")
        {
            pix_fmt = value.trim().to_string();
        }
    }

    if codec.is_empty() {
        None
    } else {
        Some((codec, pix_fmt))
    }
}

fn optimize_video(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let final_path =
        path.with_extension("mp4");

    let temporary =
        path.with_extension("optimized.mp4");

    let needs_transcode =
        match probe_video_stream(path) {
            Some((codec, pix_fmt)) => {
                codec != "h264"
                    || pix_fmt != "yuv420p"
            }

            None => false,
        };

    println!(
        "🎬 Optimizing video: {} ({})",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown"),
        if needs_transcode {
            "H.264 re-encode"
        } else {
            "MP4 remux + faststart"
        }
    );

    let mut command =
        Command::new("ffmpeg");

    command
        .arg("-y")
        .arg("-i")
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "0:a?",
            "-sn",
        ]);

    if needs_transcode {
        command.args([
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "20",
            "-pix_fmt",
            "yuv420p",
        ]);
    } else {
        command.args([
            "-c:v",
            "copy",
        ]);
    }

    let status = command
        .args([
            "-c:a",
            "aac",
            "-movflags",
            "+faststart",
        ])
        .arg(&temporary)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status();

    match status {
        Ok(status) if status.success() => {
            if let Err(error) =
                fs::rename(&temporary, &final_path)
            {
                eprintln!(
                    "❌ Failed to replace video: {error}"
                );

                let _ =
                    fs::remove_file(&temporary);

                return false;
            }

            if extension != "mp4"
                && path != final_path
            {
                if let Err(error) =
                    fs::remove_file(path)
                {
                    eprintln!(
                        "⚠️ Failed to remove original video: {error}"
                    );
                }
            }

            true
        }

        Ok(status) => {
            eprintln!(
                "❌ ffmpeg exited with {status}"
            );

            let _ =
                fs::remove_file(&temporary);

            false
        }

        Err(error) => {
            eprintln!(
                "❌ Failed to start ffmpeg: {error}"
            );

            let _ =
                fs::remove_file(&temporary);

            false
        }
    }
}

// ============================================================
// ANI-CLI TITLE
// ============================================================

fn ani_cli_search_title(title: &str) -> String {
    title
        .replace('’', "'")
        .replace('‘', "'")
        .replace('“', "\"")
        .replace('”', "\"")
        .replace('–', "-")
        .replace('—', "-")
        .replace(':', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ============================================================
// SEASON 1 SEARCH
// ============================================================

fn ani_cli_season_one_search_title(
    title: &str,
) -> String {
    let normalized =
        ani_cli_search_title(title);

    if normalized.is_empty() {
        return normalized;
    }

    format!("{normalized} Season 1")
}

// ============================================================
// DOWNLOAD ONE EPISODE
// ============================================================

fn download_episode(
    title: &str,
    episode: u32,
    season_one: bool,
    history_directory: Option<&Path>,
) -> bool {
    let directory =
        anime_directory(title);

    if let Err(error) =
        fs::create_dir_all(&directory)
    {
        eprintln!(
            "❌ Failed to create anime directory {}: {error}",
            directory.display()
        );

        return false;
    }

    println!();
    println!(
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    );

    println!("⬇️  DOWNLOADING EPISODE");
    println!("   Anime:   {title}");

    if season_one {
        println!("   Season:  1");
    }

    println!("   Episode: {episode}");

    println!(
        "   Folder:  {}",
        directory.display()
    );

    println!(
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    );

    let search_title =
        if season_one {
            ani_cli_season_one_search_title(
                title,
            )
        } else {
            ani_cli_search_title(title)
        };

    println!(
        "🔎 ani-cli search: {search_title}"
    );

    let mut command =
        Command::new("ani-cli");

    command
        .current_dir(&directory)
        .env(
            "ANI_CLI_DOWNLOAD_DIR",
            &directory,
        )
        .stdin(Stdio::null())
        .args([
            "--select-nth",
            "1",
            "-d",
            "-e",
        ])
        .arg(episode.to_string())
        .arg(search_title);

    if let Some(history_directory) =
        history_directory
    {
        command.env(
            "ANI_CLI_HIST_DIR",
            history_directory,
        );
    }

    let status = command.status();

    match status {
        Ok(status) if status.success() => {}

        Ok(status) => {
            eprintln!(
                "❌ ani-cli exited with {status}"
            );

            return false;
        }

        Err(error) => {
            eprintln!(
                "❌ Failed to start ani-cli: {error}"
            );

            return false;
        }
    }

    // --------------------------------------------------------
    // LOCATE DOWNLOADED VIDEO
    // --------------------------------------------------------

    let Some(video) =
        find_episode_video(title, episode)
    else {
        eprintln!(
            "❌ ani-cli finished but Episode {episode} \
             could not be found for {title}"
        );

        return false;
    };

    // --------------------------------------------------------
    // OPTIMIZE / NORMALIZE VIDEO
    // --------------------------------------------------------

    if !optimize_video(&video) {
        eprintln!(
            "⚠️ Video optimization failed for {}",
            video.display()
        );

        // Do not immediately fail.
        //
        // The original video may still be playable.
    }

    // --------------------------------------------------------
    // FETCH METADATA + POSTER ONCE
    // --------------------------------------------------------

    println!(
        "🖼️ Fetching AniList poster for {title}..."
    );

    let metadata =
        fetch_anilist_metadata(title);

    if let Some(metadata) = &metadata {
        if let Some(poster) =
            &metadata.poster
        {
            if !download_thumbnail(
                title,
                poster,
            ) {
                eprintln!(
                    "⚠️ Episode downloaded, but poster could not be saved for {title}"
                );
            }
        } else {
            eprintln!(
                "⚠️ AniList returned no poster URL for {title}"
            );
        }
    } else {
        eprintln!(
            "⚠️ Could not fetch AniList metadata for {title}; poster was not downloaded"
        );
    }

    // --------------------------------------------------------
    // UPDATE CATALOG ONCE
    // --------------------------------------------------------

    if !update_catalog(
        title,
        metadata.as_ref(),
    ) {
        eprintln!(
            "⚠️ Failed to update catalog for {title}"
        );

        return false;
    }

    println!(
        "✅ Finished Episode {episode}: {title}"
    );

    true
}

// ============================================================
// DOWNLOAD BRAND-NEW ANIME
// ============================================================

fn download_new_anime(
    title: &str,
) -> bool {
    let title = title.trim();

    if title.is_empty() {
        return false;
    }

    let catalog =
        read_catalog();

    if catalog_contains_title(
        &catalog,
        title,
    ) {
        println!(
            "⏭️ Already owned: {title}"
        );

        return false;
    }

    println!();
    println!(
        "╔══════════════════════════════════════════════════╗"
    );
    println!(
        "║              NEW ORION ANIME                    ║"
    );
    println!(
        "╚══════════════════════════════════════════════════╝"
    );

    println!("🎯 {title}");
    println!("📺 Season: 1");
    println!("📥 Starting with Episode 1...");

    // --------------------------------------------------------
    // DOWNLOAD SEASON 1 / EPISODE 1
    // --------------------------------------------------------

    if !download_episode(
        title,
        1,
        true,
        None,
    ) {
        eprintln!(
            "❌ Failed to download Season 1 Episode 1 for {title}"
        );

        return false;
    }

    // --------------------------------------------------------
    // FETCH METADATA
    // --------------------------------------------------------

    println!(
        "🔎 Fetching AniList metadata for {title}..."
    );

    let metadata =
        fetch_anilist_metadata(title);

    if let Some(metadata) = &metadata {
        println!("📋 Metadata found:");

        println!(
            "   Genres: {}",
            metadata.genres.join(", ")
        );

        if let Some(episodes) =
            metadata.episodes
        {
            println!(
                "   Episodes: {episodes}"
            );
        }

        if let Some(format) =
            &metadata.format
        {
            println!(
                "   Format: {format}"
            );
        }
    } else {
        println!(
            "⚠️ AniList metadata unavailable for {title}"
        );
    }

    // download_episode() already fetched the
    // poster and updated the catalog.

    if !update_catalog(
        title,
        metadata.as_ref(),
    ) {
        eprintln!(
            "⚠️ Failed to update catalog for {title}"
        );

        return false;
    }

    println!(
        "🆕 Added to Orion: {title}"
    );

    println!("📺 Season: 1");
    println!("⭐ Rating: not rated");

    true
}

// ============================================================
// CONTINUE EXISTING PRIORITY ANIME
// ============================================================

fn continue_priority_anime(
    title: &str,
) -> bool {
    let catalog =
        read_catalog();

    let Some(entry) =
        catalog.iter().find(|entry| {
            entry
                .get("title")
                .and_then(Value::as_str)
                .map(|existing| {
                    titles_match(
                        existing,
                        title,
                    )
                })
                .unwrap_or(false)
        })
    else {
        println!(
            "⚠️ Priority anime is not in catalog: {title}"
        );

        return false;
    };

    let total_episodes =
        entry
            .get("totalEpisodes")
            .and_then(Value::as_u64)
            .map(|value| value as u32);

    let next_episode =
        next_episode_number(title);

    if let Some(total) =
        total_episodes
    {
        if next_episode > total {
            println!(
                "✅ Priority anime is complete: {title}"
            );

            return false;
        }
    }

    println!();
    println!("⭐ PRIORITY DOWNLOAD");
    println!("   {title}");
    println!(
        "   Next episode: {next_episode}"
    );

    download_episode(
        title,
        next_episode,
        false,
        None,
    )
}

// ============================================================
// PRIORITY DETECTION
// ============================================================
//
// Priority comes from:
//
// 1. Orion history:
//      "_" = liked
//
// 2. catalog.json:
//      "rating": "liked"
//
// The result is deduplicated.
//
// Only anime that are actually incomplete are returned.
// ============================================================

fn incomplete_priority_anime() -> Vec<String> {
    let catalog =
        read_catalog();

    let mut candidates =
        Vec::<String>::new();

    let mut seen =
        HashSet::<String>::new();

    // --------------------------------------------------------
    // 1. ORION LIKED HISTORY
    // --------------------------------------------------------

    for title in liked_history_entries() {
        let key =
            normalize_title(&title);

        if seen.insert(key) {
            candidates.push(title);
        }
    }

    // --------------------------------------------------------
    // 2. CATALOG LIKED ENTRIES
    // --------------------------------------------------------

    for entry in &catalog {
        let is_liked =
            entry
                .get("rating")
                .and_then(Value::as_str)
                .map(|rating| {
                    rating.eq_ignore_ascii_case(
                        "liked",
                    )
                })
                .unwrap_or(false);

        if !is_liked {
            continue;
        }

        let Some(title) =
            entry
                .get("title")
                .and_then(Value::as_str)
        else {
            continue;
        };

        let key =
            normalize_title(title);

        if seen.insert(key) {
            candidates.push(
                title.to_string(),
            );
        }
    }

    // --------------------------------------------------------
    // 3. FILTER TO INCOMPLETE ANIME
    // --------------------------------------------------------

    let mut incomplete =
        Vec::new();

    for title in candidates {
        let Some(entry) =
            catalog.iter().find(|entry| {
                entry
                    .get("title")
                    .and_then(Value::as_str)
                    .map(|existing| {
                        titles_match(
                            existing,
                            &title,
                        )
                    })
                    .unwrap_or(false)
            })
        else {
            // A liked history entry that is not
            // in the catalog cannot be continued
            // as an existing anime.
            println!(
                "⚠️ Liked anime not found in catalog: {title}"
            );

            continue;
        };

        let next =
            next_episode_number(&title);

        let total =
            entry
                .get("totalEpisodes")
                .and_then(Value::as_u64)
                .map(|value| value as u32);

        let complete =
            total
                .map(|total| next > total)
                .unwrap_or(false);

        if complete {
            println!(
                "✅ Priority anime already complete: {title}"
            );

            continue;
        }

        incomplete.push(title);
    }

    incomplete
}

// ============================================================
// SEARCH
// ============================================================

pub fn search_anime(
    query: &str,
) -> Result<Value, String> {
    let response =
        anilist_request(
            r#"
            query ($search: String) {
                Page(perPage: 12) {
                    media(
                        search: $search,
                        type: ANIME,
                        sort: SEARCH_MATCH
                    ) {
                        id

                        title {
                            romaji
                            english
                            native
                        }

                        episodes
                        format
                        seasonYear
                        description(asHtml: false)

                        coverImage {
                            extraLarge
                            large
                        }
                    }
                }
            }
        "#,
            json!({
                "search": query.trim()
            }),
        )
        .ok_or_else(|| {
            "AniList search failed".to_string()
        })?;

    response
        .get("data")
        .and_then(|data| data.get("Page"))
        .and_then(|page| page.get("media"))
        .cloned()
        .ok_or_else(|| {
            "AniList returned no search results"
                .to_string()
        })
}

// ============================================================
// MANUAL EPISODE DOWNLOAD
// ============================================================

pub fn download_manual_episode(
    title: &str,
    episode: u32,
    total_episodes: Option<u32>,
    poster_url: Option<&str>,
) -> bool {
    if title.trim().is_empty()
        || episode == 0
        || total_episodes
            .is_some_and(|total| episode > total)
    {
        return false;
    }

    let history_directory =
        project_root()
            .join("src-tauri")
            .join("state")
            .join("manual-ani-cli-history");

    if fs::create_dir_all(
        &history_directory,
    )
    .is_err()
    {
        return false;
    }

    if !download_episode(
        title.trim(),
        episode,
        false,
        Some(&history_directory),
    ) {
        return false;
    }

    let metadata =
        fetch_anilist_metadata(
            title.trim(),
        );

    let selected_poster =
        poster_url.filter(|url| {
            url.starts_with("https://")
                && (
                    url.contains("anilist.co")
                        || url.contains("anili.st")
                )
        });

    let metadata_poster =
        metadata
            .as_ref()
            .and_then(|metadata| {
                metadata.poster.as_deref()
            });

    if let Some(poster_url) =
        selected_poster.or(metadata_poster)
    {
        if !download_thumbnail(
            title.trim(),
            poster_url,
        ) {
            eprintln!(
                "❌ Could not save AniList poster to {}",
                poster_directory(title.trim())
                    .join("poster.jpg")
                    .display()
            );
        }
    }

    update_catalog(
        title.trim(),
        metadata.as_ref(),
    )
}

// ============================================================
// RECOMMENDATION FILTERING
// ============================================================

#[allow(dead_code)]
fn recommendation_is_owned(
    title: &str,
) -> bool {
    let catalog =
        read_catalog();

    catalog_contains_title(
        &catalog,
        title,
    )
}

fn filter_new_recommendations(
    recommendations: Vec<String>,
) -> Vec<String> {
    let catalog =
        read_catalog();

    let mut seen =
        HashSet::new();

    let mut filtered =
        Vec::new();

    for title in recommendations {
        let title =
            title.trim();

        if title.is_empty() {
            continue;
        }

        let key =
            normalize_title(title);

        if !seen.insert(key) {
            continue;
        }

        if catalog_contains_title(
            &catalog,
            title,
        ) {
            println!(
                "⏭️ Recommendation already owned: {title}"
            );

            continue;
        }

        filtered.push(
            title.to_string(),
        );
    }

    filtered
}

// ============================================================
// DOWNLOAD RECOMMENDATIONS
// ============================================================
//
// PHASE 1:
//
// Process EVERY incomplete priority anime.
//
// A -> next episode
// B -> next episode
// C -> next episode
//
// PHASE 2:
//
// After ALL priority anime have been processed:
//
// Download up to 5 NEW recommendations.
//
// ============================================================

fn download_recommendations(
    recommendations: Vec<String>,
) {
    let catalog =
        read_catalog();

    let current_count =
        catalog.len();

    println!();

    println!(
        "╔══════════════════════════════════════════════════╗"
    );

    println!(
        "║              ORION GENERATION                   ║"
    );

    println!(
        "╚══════════════════════════════════════════════════╝"
    );

    println!(
        "📚 Current library: {current_count}/{LIBRARY_LIMIT}"
    );

    // ========================================================
    // PHASE 1: EVERY PRIORITY ANIME
    // ========================================================

    let priority =
        incomplete_priority_anime();

    if priority.is_empty() {
        println!();
        println!(
            "✅ No incomplete priority anime."
        );
    } else {
        println!();

        println!(
            "⭐ Priority anime count: {}",
            priority.len()
        );

        println!(
            "⭐ Processing EVERY priority anime..."
        );

        let mut priority_success =
            0usize;

        let mut priority_failed =
            0usize;

        for (index, title) in
            priority.iter().enumerate()
        {
            println!();

            println!(
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
            );

            println!(
                "⭐ PRIORITY {}/{}: {}",
                index + 1,
                priority.len(),
                title
            );

            if continue_priority_anime(
                title,
            ) {
                priority_success += 1;

                println!(
                    "✅ Priority episode downloaded: {title}"
                );
            } else {
                priority_failed += 1;

                println!(
                    "⚠️ Priority episode failed/completed: {title}"
                );
            }

            println!(
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
            );
        }

        println!();

        println!(
            "⭐ Priority processing complete."
        );

        println!(
            "   ✅ Successful: {priority_success}"
        );

        println!(
            "   ⚠️ Failed/skipped: {priority_failed}"
        );
    }

    // ========================================================
    // PHASE 2: CHECK LIBRARY LIMIT
    // ========================================================

    let current_count =
        read_catalog().len();

    if current_count >= LIBRARY_LIMIT {
        println!();

        println!("⚠️ Library is full.");

        println!(
            "📚 {current_count}/{LIBRARY_LIMIT}"
        );

        println!(
            "ℹ️ No new anime can be added."
        );

        return;
    }

    // ========================================================
    // PHASE 3: NEW RECOMMENDATIONS
    // ========================================================

    let available_slots =
        LIBRARY_LIMIT - current_count;

    let recommendation_slots =
        available_slots.min(
            NEW_ANIME_PER_GENERATION,
        );

    println!();

    println!(
        "📦 Available library slots: {available_slots}"
    );

    println!(
        "🎯 New recommendation slots: {recommendation_slots}"
    );

    let recommendations =
        filter_new_recommendations(
            recommendations,
        );

    if recommendations.is_empty() {
        println!(
            "ℹ️ No new recommendations available."
        );

        println!();

        println!(
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        );

        println!(
            "📊 Generation finished."
        );

        println!(
            "📚 Library now: {}/{}",
            read_catalog().len(),
            LIBRARY_LIMIT
        );

        println!(
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        );

        return;
    }

    // ========================================================
    // DOWNLOAD UP TO FIVE NEW ANIME
    // ========================================================

    let mut added =
        0usize;

    for title in recommendations {
        if added >= recommendation_slots {
            break;
        }

        let latest_catalog =
            read_catalog();

        if latest_catalog.len()
            >= LIBRARY_LIMIT
        {
            println!(
                "⚠️ Library reached {LIBRARY_LIMIT}/{LIBRARY_LIMIT}."
            );

            break;
        }

        if catalog_contains_title(
            &latest_catalog,
            &title,
        ) {
            println!(
                "⏭️ Already owned: {title}"
            );

            continue;
        }

        println!();

        println!(
            "🎯 Recommendation {}/{}: {}",
            added + 1,
            recommendation_slots,
            title
        );

        if download_new_anime(
            &title,
        ) {
            added += 1;

            println!(
                "✅ Recommendation added ({}/{})",
                added,
                recommendation_slots
            );
        } else {
            println!(
                "⚠️ Failed to add recommendation: {title}"
            );
        }
    }

    // ========================================================
    // GENERATION SUMMARY
    // ========================================================

    println!();

    println!(
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    );

    println!(
        "📊 Generation finished."
    );

    println!(
        "🆕 New anime added: {added}"
    );

    println!(
        "📚 Library now: {}/{}",
        read_catalog().len(),
        LIBRARY_LIMIT
    );

    println!(
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    );
}

// ============================================================
// PREDICTION INTEGRATION
// ============================================================

pub fn download_predicted_anime() {
    println!();

    println!(
        "🔮 Running Orion prediction model..."
    );

    let recommendations =
        prediction::get_recommendations();

    if recommendations.is_empty() {
        println!(
            "⚠️ Prediction model returned no recommendations."
        );

        return;
    }

    println!(
        "🧠 Prediction model returned {} recommendations.",
        recommendations.len()
    );

    download_recommendations(
        recommendations,
    );

    println!();

    println!(
        "✅ PREDICTION DOWNLOAD FINISHED"
    );
}

// ============================================================
// GENERATION ENTRY POINT
// ============================================================

#[allow(dead_code)]
pub fn try_generate() -> bool {
    let Some(_guard) =
        try_begin_generation()
    else {
        println!(
            "⚠️ Generation already running."
        );

        return false;
    };

    download_predicted_anime();

    true
}

// ============================================================
// PUBLIC HELPERS
// ============================================================

#[allow(dead_code)]
pub fn library_count() -> usize {
    read_catalog().len()
}

#[allow(dead_code)]
pub fn library_is_full() -> bool {
    library_count() >= LIBRARY_LIMIT
}

#[allow(dead_code)]
pub fn is_liked(title: &str) -> bool {
    liked_history_contains(title)
}

#[allow(dead_code)]
pub fn next_episode(title: &str) -> u32 {
    next_episode_number(title)
}

#[allow(dead_code)]
pub fn owned_titles() -> Vec<String> {
    read_catalog()
        .into_iter()
        .filter_map(|entry| {
            entry
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

// ============================================================
// DEBUG / DEVELOPMENT
// ============================================================

#[allow(dead_code)]
fn print_priority_debug() {
    println!();

    println!(
        "========== ORION PRIORITY DEBUG =========="
    );

    let history =
        read_orion_history();

    if history.is_empty() {
        println!(
            "No Orion history found."
        );
    } else {
        println!("Orion history:");

        for entry in &history {
            println!(
                "  episode={} marker={} title={}",
                entry.episode,
                entry.marker,
                entry.title
            );
        }
    }

    let liked =
        liked_history_entries();

    println!();

    println!("Liked anime:");

    for title in liked {
        println!(
            "  ❤️ {title}"
        );
    }

    let priority =
        incomplete_priority_anime();

    println!();

    println!(
        "Incomplete priority anime:"
    );

    for title in priority {
        println!(
            "  ⭐ {title}"
        );
    }

    println!(
        "==========================================="
    );
}

#[allow(dead_code)]
fn _history_summary() -> HashMap<String, u32> {
    let mut result =
        HashMap::new();

    for entry in read_orion_history() {
        result.insert(
            normalize_title(&entry.title),
            entry.episode,
        );
    }

    result
}

#[allow(dead_code)]
fn api_delay(milliseconds: u64) {
    thread::sleep(
        Duration::from_millis(milliseconds),
    );
}

#[allow(dead_code)]
fn _io_type_check(_: io::Result<()>) {}

