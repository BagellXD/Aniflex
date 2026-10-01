use std::{
collections::{HashMap, HashSet},
fs,
io::{BufRead, BufReader, Read},
path::PathBuf,
sync::{Mutex, OnceLock},
thread,
time::{Duration, Instant},
};

use serde_json::{json, Value};

// ============================================================
// CONFIGURATION
// ============================================================

const ANILIST_API_URL: &str = "https://graphql.anilist.co";

// AniList request pacing.
//
// This intentionally favors a deep search over a fast search.
// Orion is supposed to examine a large number of candidates.
const REQUEST_DELAY_MS: u64 = 2100;
const REQUESTS_PER_MINUTE: usize = 29;

// Search depth.
//
// First hop:
//   History -> recommendations/relations
//
// Second hop:
//   Strong first-hop candidates -> their recommendations/relations
//
// This creates a much larger candidate graph before scoring.
const MAX_FIRST_HOP_CANDIDATES: usize = 500;
const SECOND_HOP_SEEDS: usize = 150;
const MAX_DEEP_CANDIDATES: usize = 1200;

// Number of final recommendations.
const MAX_RESULTS: usize = 10;

// AniList recommendations requested for each anime.
const RECOMMENDATIONS_PER_ANIME: usize = 25;

// AniList can return many IDs in a single id_in query.
const ANILIST_BATCH_SIZE: usize = 50;

// ============================================================
// REQUEST LIMITER
// ============================================================

struct RequestWindow {
started_at: Instant,
request_count: usize,
last_request_at: Option<Instant>,
}

static ANILIST_REQUEST_WINDOW: OnceLock<Mutex<RequestWindow>> = OnceLock::new();

fn wait_for_anilist() {
let limiter = ANILIST_REQUEST_WINDOW.get_or_init(|| {
Mutex::new(RequestWindow {
started_at: Instant::now(),
request_count: 0,
last_request_at: None,
})
});


let mut window = limiter
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());

loop {
    let now = Instant::now();

    if now.duration_since(window.started_at) >= Duration::from_secs(60) {
        window.started_at = now;
        window.request_count = 0;
        window.last_request_at = None;
    }

    if window.request_count >= REQUESTS_PER_MINUTE {
        let elapsed = Instant::now().duration_since(window.started_at);
        let remaining = Duration::from_secs(60).saturating_sub(elapsed);

        println!();
        println!("⏳ AniList request limit reached.");
        println!(
            "   Waiting approximately {} seconds...",
            remaining.as_secs().saturating_add(1)
        );
        println!();

        drop(window);
        thread::sleep(remaining);

        window = limiter
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        window.started_at = Instant::now();
        window.request_count = 0;
        window.last_request_at = None;

        continue;
    }

    if let Some(last) = window.last_request_at {
        let elapsed = now.duration_since(last);
        let minimum_gap = Duration::from_millis(REQUEST_DELAY_MS);

        if elapsed < minimum_gap {
            let remaining = minimum_gap - elapsed;

            drop(window);
            thread::sleep(remaining);

            window = limiter
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            continue;
        }
    }

    window.request_count += 1;
    window.last_request_at = Some(Instant::now());

    return;
}


}

// ============================================================
// PATHS
// ============================================================

fn project_root() -> PathBuf {
let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));


manifest
    .parent()
    .map(PathBuf::from)
    .unwrap_or(manifest)


}

fn catalog_path() -> PathBuf {
project_root().join("data").join("catalog.json")
}

fn ani_cli_history_path() -> PathBuf {
if let Ok(path) = std::env::var("ANI_CLI_HIST_DIR") {
return PathBuf::from(path).join("ani-hsts");
}


if let Ok(path) = std::env::var("XDG_STATE_HOME") {
    return PathBuf::from(path).join("ani-cli").join("ani-hsts");
}

if let Ok(home) = std::env::var("HOME") {
    return PathBuf::from(home)
        .join(".local")
        .join("state")
        .join("ani-cli")
        .join("ani-hsts");
}

PathBuf::from("/home/bobisaac/.local/state/ani-cli/ani-hsts")


}

// ============================================================
// TITLE NORMALIZATION
// ============================================================

fn normalize_title(title: &str) -> String {
title
.to_lowercase()
.chars()
.map(|character| {
if character.is_alphanumeric() || character.is_whitespace() {
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

// ============================================================
// LOCAL LIBRARY
// ============================================================

fn read_owned_titles() -> HashSet<String> {
let path = catalog_path();


println!("📚 Reading Orion local catalog:");
println!("   {}", path.display());
println!();

let contents = match fs::read_to_string(&path) {
    Ok(contents) => contents,
    Err(error) => {
        println!("⚠️ Could not read catalog: {}", error);
        println!("   Continuing with an empty local library.");
        println!();
        return HashSet::new();
    }
};

let value: Value = match serde_json::from_str(&contents) {
    Ok(value) => value,
    Err(error) => {
        println!("⚠️ Could not parse catalog: {}", error);
        return HashSet::new();
    }
};

let mut owned = HashSet::new();

let Some(entries) = value.as_array() else {
    println!("⚠️ catalog.json is not an array.");
    return owned;
};

for entry in entries {
    let Some(title) = entry.get("title").and_then(Value::as_str) else {
        continue;
    };

    let normalized = normalize_title(title);

    if !normalized.is_empty() {
        owned.insert(normalized);
    }
}

println!("📚 Orion currently owns {} anime.", owned.len());
println!();

owned


}

// ============================================================
// ANI-CLI HISTORY
// ============================================================

fn read_ani_cli_history() -> Vec<String> {
let path = ani_cli_history_path();


println!("📂 Reading ani-cli history:");
println!("   {}", path.display());
println!();

let file = match fs::File::open(&path) {
    Ok(file) => Some(file),
    Err(error) => {
        eprintln!("⚠️ Could not open ani-cli history: {}", error);
        None
    }
};

let mut history = Vec::new();
let mut seen = HashSet::new();
let liked_titles: Vec<String> = fs::read_to_string(catalog_path())
    .ok()
    .and_then(|contents| serde_json::from_str::<Value>(&contents).ok())
    .and_then(|catalog| catalog.as_array().cloned())
    .unwrap_or_default()
    .iter()
    .filter(|entry| entry.get("rating").and_then(Value::as_str) == Some("liked"))
    .filter_map(|entry| entry.get("title").and_then(Value::as_str))
    .map(str::to_string)
    .collect();

if let Some(file) = file {
for line in BufReader::new(file).lines() {
    let Ok(line) = line else {
        continue;
    };

    let line = line.trim();

    if line.is_empty() {
        continue;
    }

    let Some((_, title)) = line.rsplit_once('\t') else {
        continue;
    };

    let title = title.trim().to_string();

    if title.is_empty() {
        continue;
    }

    let key = normalize_title(&title);

    if seen.insert(key) {
        history.push(title);
    }
}
}

for title in liked_titles {
    let key = normalize_title(&title);

    if seen.insert(key) {
        println!("   ❤️ Adding liked anime as prediction input: {title}");
        history.push(title);
    }
}

println!(
    "📂 Loaded {} unique prediction taste inputs.",
    history.len()
);

for title in &history {
    println!("   • {}", title);
}

println!();

history


}

// ============================================================
// ANILIST DATA STRUCTURES
// ============================================================

#[derive(Debug, Clone)]
struct Anime {
id: i32,
anime_type: Option<String>,
start_year: Option<i32>,
description: Option<String>,
titles: Titles,
genres: Vec<String>,
tags: Vec<Tag>,
recommendations: Vec<AnimeReference>,
relations: Vec<RelatedAnimeReference>,
}

#[derive(Debug, Clone)]
struct Titles {
entries: Vec<Title>,
}

#[derive(Debug, Clone)]
struct Title {
text: String,
title_type: String,
}

#[derive(Debug, Clone)]
struct Tag {
name: String,
rank: f32,
}

#[derive(Debug, Clone)]
struct AnimeReference {
id: i32,
title: String,
}

#[derive(Debug, Clone)]
struct RelatedAnimeReference {
id: i32,
title: String,
relation_type: String,
}

// ============================================================
// ANILIST GRAPHQL
// ============================================================

const ANILIST_FIELDS: &str = r#"
fragment AnimeFields on Media {
id
format


startDate {
    year
}

description(asHtml: false)

title {
    romaji
    english
    native
}

genres

tags {
    name
    rank
    isMediaSpoiler
}

relations {
    edges {
        relationType

        node {
            id
            type

            title {
                romaji
                english
                native
            }
        }
    }
}

recommendations(
    page: 1
    perPage: 25
    sort: RATING_DESC
) {
    nodes {
        mediaRecommendation {
            id
            type

            title {
                romaji
                english
                native
            }
        }
    }
}


}
"#;

const SEARCH_QUERY: &str = r#"
query ($search: String) {
Media(
search: $search
type: ANIME
isAdult: false
) {
...AnimeFields
}
}
"#;

const ID_QUERY: &str = r#"
query ($id: Int) {
Media(
id: $id
type: ANIME
isAdult: false
) {
...AnimeFields
}
}
"#;

const BATCH_QUERY: &str = r#"
query ($ids: [Int]) {
Page(
page: 1
perPage: 50
) {
media(
id_in: $ids
type: ANIME
isAdult: false
) {
...AnimeFields
}
}
}
"#;

// ============================================================
// DISPLAY TITLE
// ============================================================

fn title_from_json(media: &Value) -> String {
media
.pointer("/title/english")
.and_then(Value::as_str)
.or_else(|| {
media
.pointer("/title/romaji")
.and_then(Value::as_str)
})
.or_else(|| {
media
.pointer("/title/native")
.and_then(Value::as_str)
})
.unwrap_or("Unknown anime")
.to_string()
}

// ============================================================
// PARSE ANILIST ANIME
// ============================================================

fn parse_anime(media: &Value) -> Option<Anime> {
let id = i32::try_from(media.get("id")?.as_i64()?).ok()?;


let anime_type = media
    .get("format")
    .and_then(Value::as_str)
    .map(str::to_string);

let start_year = media
    .pointer("/startDate/year")
    .and_then(Value::as_i64)
    .and_then(|year| i32::try_from(year).ok());

let description = media
    .get("description")
    .and_then(Value::as_str)
    .map(str::to_string);

// --------------------------------------------------------
// TITLES
// --------------------------------------------------------

let mut titles = Vec::new();

if let Some(title) = media.get("title") {
    for (field, title_type) in [
        ("english", "english"),
        ("romaji", "romaji"),
        ("native", "native"),
    ] {
        if let Some(text) = title.get(field).and_then(Value::as_str) {
            if !text.is_empty() {
                titles.push(Title {
                    text: text.to_string(),
                    title_type: title_type.to_string(),
                });
            }
        }
    }
}

// --------------------------------------------------------
// GENRES
// --------------------------------------------------------

let genres = media
    .get("genres")
    .and_then(Value::as_array)
    .map(|genres| {
        genres
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default();

// --------------------------------------------------------
// TAGS
// --------------------------------------------------------

let tags = media
    .get("tags")
    .and_then(Value::as_array)
    .map(|items| {
        items
            .iter()
            .filter_map(|tag| {
                let name = tag.get("name")?.as_str()?;

                let spoiler = tag
                    .get("isMediaSpoiler")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);

                if spoiler {
                    return None;
                }

                let rank = tag
                    .get("rank")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as f32;

                Some(Tag {
                    name: name.to_string(),
                    rank,
                })
            })
            .collect()
    })
    .unwrap_or_default();

// --------------------------------------------------------
// RELATIONS
// --------------------------------------------------------

let relations = media
    .pointer("/relations/edges")
    .and_then(Value::as_array)
    .map(|edges| {
        edges
            .iter()
            .filter_map(|edge| {
                let relation_type =
                    edge.get("relationType")?.as_str()?;

                let node = edge.get("node")?;

                if node.get("type").and_then(Value::as_str) != Some("ANIME") {
                    return None;
                }

                let id = i32::try_from(
                    node.get("id")?.as_i64()?,
                )
                .ok()?;

                Some(RelatedAnimeReference {
                    id,
                    title: title_from_json(node),
                    relation_type: relation_type.to_string(),
                })
            })
            .collect()
    })
    .unwrap_or_default();

// --------------------------------------------------------
// RECOMMENDATIONS
// --------------------------------------------------------

let recommendations = media
    .pointer("/recommendations/nodes")
    .and_then(Value::as_array)
    .map(|nodes| {
        nodes
            .iter()
            .filter_map(|node| {
                let recommendation =
                    node.get("mediaRecommendation")?;

                if recommendation
                    .get("type")
                    .and_then(Value::as_str)
                    != Some("ANIME")
                {
                    return None;
                }

                let id = i32::try_from(
                    recommendation.get("id")?.as_i64()?,
                )
                .ok()?;

                Some(AnimeReference {
                    id,
                    title: title_from_json(recommendation),
                })
            })
            .collect()
    })
    .unwrap_or_default();

Some(Anime {
    id,
    anime_type,
    start_year,
    description,
    titles: Titles { entries: titles },
    genres,
    tags,
    recommendations,
    relations,
})


}

// ============================================================
// ANILIST REQUEST
// ============================================================

fn post_anilist(payload: Value) -> Option<Value> {
wait_for_anilist();


let body = serde_json::to_string(&payload).ok()?;

let response = match ureq::post(ANILIST_API_URL)
    .set("Content-Type", "application/json")
    .set("Accept", "application/json")
    .send_string(&body)
{
    Ok(response) => response,
    Err(error) => {
        eprintln!("❌ AniList request failed: {}", error);
        return None;
    }
};

let mut response_body = String::new();

if response
    .into_reader()
    .read_to_string(&mut response_body)
    .is_err()
{
    return None;
}

let json: Value = match serde_json::from_str(&response_body) {
    Ok(value) => value,
    Err(error) => {
        eprintln!("❌ Invalid AniList JSON: {}", error);
        return None;
    }
};

if let Some(errors) = json.get("errors").and_then(Value::as_array) {
    for error in errors {
        if let Some(message) =
            error.get("message").and_then(Value::as_str)
        {
            eprintln!("❌ AniList API error: {}", message);
        }
    }

    return None;
}

Some(json)


}

// ============================================================
// SEARCH ONE ANIME
// ============================================================

fn search_anime(title: &str) -> Option<Anime> {
let payload = json!({
"query": format!("{}\n{}", SEARCH_QUERY, ANILIST_FIELDS),
"variables": {
"search": title
}
});


let response = post_anilist(payload)?;

let media = response.pointer("/data/Media")?;

parse_anime(media)


}

// ============================================================
// FETCH ONE ANIME BY ID
// ============================================================

fn fetch_anime(id: i32) -> Option<Anime> {
let payload = json!({
"query": format!("{}\n{}", ID_QUERY, ANILIST_FIELDS),
"variables": {
"id": id
}
});


let response = post_anilist(payload)?;

let media = response.pointer("/data/Media")?;

if media.is_null() {
    return None;
}

parse_anime(media)


}

// ============================================================
// BATCH FETCH
// ============================================================

fn fetch_anime_batch(ids: &[i32]) -> HashMap<i32, Anime> {
let mut result = HashMap::new();


if ids.is_empty() {
    return result;
}

let payload = json!({
    "query": format!("{}\n{}", BATCH_QUERY, ANILIST_FIELDS),
    "variables": {
        "ids": ids
    }
});

let Some(response) = post_anilist(payload) else {
    return result;
};

let Some(media) = response
    .pointer("/data/Page/media")
    .and_then(Value::as_array)
else {
    return result;
};

for item in media {
    if let Some(anime) = parse_anime(item) {
        result.insert(anime.id, anime);
    }
}

result


}

// ============================================================
// ENSURE METADATA
// ============================================================

fn ensure_anime_loaded(
ids: &[i32],
cache: &mut HashMap<i32, Anime>,
) {
let missing: Vec<i32> = ids
.iter()
.copied()
.filter(|id| !cache.contains_key(id))
.collect();


for batch in missing.chunks(ANILIST_BATCH_SIZE) {
    println!(
        "🌐 Fetching metadata for {} anime...",
        batch.len()
    );

    let fetched = fetch_anime_batch(batch);

    for (id, anime) in fetched {
        cache.insert(id, anime);
    }
}


}

// ============================================================
// DISPLAY TITLE
// ============================================================

fn get_display_title(anime: &Anime) -> String {
anime
.titles
.entries
.iter()
.find(|title| title.title_type == "english")
.or_else(|| {
anime
.titles
.entries
.iter()
.find(|title| title.title_type == "romaji")
})
.or_else(|| anime.titles.entries.first())
.map(|title| title.text.clone())
.unwrap_or_else(|| format!("AniList ID {}", anime.id))
}

// ============================================================
// LOCAL OWNERSHIP CHECK
// ============================================================

fn anime_matches_owned_title(
anime: &Anime,
owned_titles: &HashSet<String>,
) -> bool {
anime.titles.entries.iter().any(|title| {
owned_titles.contains(&normalize_title(&title.text))
})
}

// ============================================================
// RELATIONSHIP HELPERS
// ============================================================

fn prequels(anime: &Anime) -> Vec<RelatedAnimeReference> {
anime
.relations
.iter()
.filter(|relation| relation.relation_type == "PREQUEL")
.cloned()
.collect()
}

// ============================================================
// FIRST-SEASON RESOLUTION
// ============================================================

#[derive(Debug)]
enum ResolutionResult {
Recommend(Anime),


AlreadyOwned {
    title: String,
},

Watched {
    title: String,
},

Invalid,


}

fn resolve_first_season(
    starting_anime: Anime,
    cache: &mut HashMap<i32, Anime>,
    watched_ids: &HashSet<i32>,
    owned_titles: &HashSet<String>,
) -> ResolutionResult {
    println!();
    println!("   🔍 Checking: {}", get_display_title(&starting_anime));
    println!("      AniList ID: {}", starting_anime.id);

    let mut current = starting_anime;
    let mut visited = HashSet::new();

    loop {
        if !visited.insert(current.id) {
            println!("      ⚠️ Relation loop detected.");
            return ResolutionResult::Invalid;
        }

        let current_title = get_display_title(&current);

        println!("      🎬 Current entry: {}", current_title);

        // ----------------------------------------------------
        // CHECK WHETHER THIS ENTRY IS ALREADY WATCHED
        // ----------------------------------------------------

        if watched_ids.contains(&current.id) {
            println!("      ❌ Already in ani-cli history.");

            return ResolutionResult::Watched {
                title: current_title,
            };
        }

        // ----------------------------------------------------
        // CHECK WHETHER THIS ENTRY IS ALREADY OWNED
        // ----------------------------------------------------

        if anime_matches_owned_title(&current, owned_titles) {
            println!("      ❌ Orion already owns this anime.");

            return ResolutionResult::AlreadyOwned {
                title: current_title,
            };
        }

        // ----------------------------------------------------
        // FIND ALL PREQUELS
        // ----------------------------------------------------

        let mut prequel_candidates = prequels(&current);

        if prequel_candidates.is_empty() {
            println!("      ✅ This is the earliest/main entry.");
            return ResolutionResult::Recommend(current);
        }

        println!(
            "      ↩️ Found {} possible prequel(s).",
            prequel_candidates.len()
        );

        // ----------------------------------------------------
        // LOAD EVERY PREQUEL
        // ----------------------------------------------------

        let mut loaded_prequels = Vec::new();

        for prequel in prequel_candidates.drain(..) {
            let next = if let Some(cached) = cache.get(&prequel.id) {
                println!(
                    "      💾 Prequel already cached: {}",
                    get_display_title(cached)
                );

                cached.clone()
            } else {
                println!(
                    "      🌐 Fetching prequel: {}",
                    prequel.title
                );

                let Some(fetched) = fetch_anime(prequel.id) else {
                    println!(
                        "      ⚠️ Could not fetch prequel: {}",
                        prequel.title
                    );

                    continue;
                };

                cache.insert(fetched.id, fetched.clone());

                fetched
            };

            loaded_prequels.push(next);
        }

        if loaded_prequels.is_empty() {
            println!(
                "      ⚠️ No prequel metadata could be loaded."
            );

            println!(
                "      ✅ Keeping current entry: {}",
                current_title
            );

            return ResolutionResult::Recommend(current);
        }

        // ----------------------------------------------------
        // SELECT THE EARLIEST PREQUEL
        //
        // AniList does not expose a simple "season number"
        // that reliably means Season 1 / Season 2 / Season 3
        // across every franchise.
        //
        // Therefore we use release year as the primary
        // chronological signal.
        // ----------------------------------------------------

        loaded_prequels.sort_by(|left, right| {
            match (left.start_year, right.start_year) {
                (Some(left_year), Some(right_year)) => {
                    left_year.cmp(&right_year)
                }

                (Some(_), None) => std::cmp::Ordering::Less,

                (None, Some(_)) => std::cmp::Ordering::Greater,

                (None, None) => {
                    get_display_title(left)
                        .cmp(&get_display_title(right))
                }
            }
        });

        let next = loaded_prequels
            .into_iter()
            .next()
            .expect("loaded_prequels cannot be empty");

        println!(
            "      📌 Selecting earliest prequel: {}",
            get_display_title(&next)
        );

        if let Some(year) = next.start_year {
            println!("      📅 Release year: {}", year);
        }

        // ----------------------------------------------------
        // KEEP WALKING BACKWARD
        // ----------------------------------------------------

        current = next;
    }
}

// ============================================================
// HISTORY RESOLUTION
// ============================================================

fn resolve_history(
titles: &[String],
cache: &mut HashMap<i32, Anime>,
) -> Vec<Anime> {
println!(
"🧠 Resolving {} history entries...",
titles.len()
);


println!();

let mut history = Vec::new();
let mut seen_ids = HashSet::new();

for title in titles {
    println!("🔎 History lookup: {}", title);

    let Some(anime) = search_anime(title) else {
        println!("   ❌ Could not resolve.");
        println!();
        continue;
    };

    println!(
        "   ✅ {} (AniList ID {})",
        get_display_title(&anime),
        anime.id
    );

    if seen_ids.insert(anime.id) {
        cache.insert(anime.id, anime.clone());
        history.push(anime);
    }

    println!();
}

history


}

// ============================================================
// CANDIDATE EVIDENCE
// ============================================================

#[derive(Debug, Clone)]
struct CandidateEvidence {
recommendation_sources: HashSet<i32>,
relation_sources: HashSet<i32>,
deep_sources: HashSet<i32>,
title: String,
}

impl CandidateEvidence {
fn new(title: String) -> Self {
Self {
recommendation_sources: HashSet::new(),
relation_sources: HashSet::new(),
deep_sources: HashSet::new(),
title,
}
}


fn strength(&self) -> usize {
    self.recommendation_sources.len()
        + self.relation_sources.len()
        + self.deep_sources.len()
}


}

// ============================================================
// ADD CANDIDATE
// ============================================================

fn add_candidate(
candidates: &mut HashMap<i32, CandidateEvidence>,
id: i32,
title: String,
source_id: i32,
source_type: &str,
deep: bool,
) {
let evidence = candidates
.entry(id)
.or_insert_with(|| CandidateEvidence::new(title.clone()));


if evidence.title.is_empty() {
    evidence.title = title;
}

if deep {
    evidence.deep_sources.insert(source_id);
    return;
}

match source_type {
    "recommendation" => {
        evidence.recommendation_sources.insert(source_id);
    }

    "relation" => {
        evidence.relation_sources.insert(source_id);
    }

    _ => {}
}


}

// ============================================================
// FIRST-HOP CANDIDATES
// ============================================================

fn build_first_hop_candidates(
history: &[Anime],
watched_ids: &HashSet<i32>,
) -> HashMap<i32, CandidateEvidence> {
println!("🌐 FIRST-HOP SEARCH");


println!(
    "   Searching recommendations and relations from ALL history."
);

println!();

let mut candidates = HashMap::new();

for anime in history {
    println!(
        "   🔎 Expanding: {}",
        get_display_title(anime)
    );

    // ----------------------------------------------------
    // RECOMMENDATIONS
    // ----------------------------------------------------

    for recommendation in &anime.recommendations {
        if watched_ids.contains(&recommendation.id) {
            continue;
        }

        add_candidate(
            &mut candidates,
            recommendation.id,
            recommendation.title.clone(),
            anime.id,
            "recommendation",
            false,
        );
    }

    // ----------------------------------------------------
    // RELATIONS
    // ----------------------------------------------------

    for relation in &anime.relations {
        if watched_ids.contains(&relation.id) {
            continue;
        }

        add_candidate(
            &mut candidates,
            relation.id,
            relation.title.clone(),
            anime.id,
            "relation",
            false,
        );
    }
}

println!();

println!(
    "🧪 First-hop candidate count: {}",
    candidates.len()
);

candidates


}

// ============================================================
// SECOND-HOP CANDIDATES
// ============================================================

fn build_second_hop_candidates(
candidates: &mut HashMap<i32, CandidateEvidence>,
cache: &mut HashMap<i32, Anime>,
watched_ids: &HashSet<i32>,
) {
println!();
println!("════════════════════════════════════");
println!("🧬 SECOND-HOP DEEP SEARCH");
println!("════════════════════════════════════");
println!();


// --------------------------------------------------------
// Rank first-hop candidates by graph strength.
// --------------------------------------------------------

let mut seeds: Vec<(i32, usize)> = candidates
    .iter()
    .map(|(id, evidence)| (*id, evidence.strength()))
    .collect();

seeds.sort_by(|left, right| right.1.cmp(&left.1));

seeds.truncate(SECOND_HOP_SEEDS);

let seed_ids: Vec<i32> =
    seeds.iter().map(|(id, _)| *id).collect();

ensure_anime_loaded(&seed_ids, cache);

println!(
    "🌱 Expanding {} strong candidates.",
    seed_ids.len()
);

println!();

for (index, seed_id) in seed_ids.iter().enumerate() {
    let Some(seed) = cache.get(seed_id) else {
        continue;
    };

    println!(
        "   [{}/{}] 🔬 Deep checking: {}",
        index + 1,
        seed_ids.len(),
        get_display_title(seed)
    );

    // ----------------------------------------------------
    // RECOMMENDATIONS OF THE SEED
    // ----------------------------------------------------

    for recommendation in &seed.recommendations {
        if watched_ids.contains(&recommendation.id) {
            continue;
        }

        add_candidate(
            candidates,
            recommendation.id,
            recommendation.title.clone(),
            *seed_id,
            "recommendation",
            true,
        );

        if candidates.len() >= MAX_DEEP_CANDIDATES {
            break;
        }
    }

    if candidates.len() >= MAX_DEEP_CANDIDATES {
        break;
    }

    // ----------------------------------------------------
    // RELATIONS OF THE SEED
    // ----------------------------------------------------

    for relation in &seed.relations {
        if watched_ids.contains(&relation.id) {
            continue;
        }

        add_candidate(
            candidates,
            relation.id,
            relation.title.clone(),
            *seed_id,
            "relation",
            true,
        );

        if candidates.len() >= MAX_DEEP_CANDIDATES {
            break;
        }
    }

    if candidates.len() >= MAX_DEEP_CANDIDATES {
        break;
    }
}

println!();

println!(
    "🧪 Deep candidate pool: {}",
    candidates.len()
);


}

// ============================================================
// KEYWORDS
// ============================================================

fn extract_keywords(text: &str) -> HashSet<String> {
const STOPWORDS: &[&str] = &[
"the",
"and",
"that",
"this",
"with",
"from",
"have",
"will",
"their",
"there",
"which",
"when",
"where",
"about",
"into",
"after",
"before",
"while",
"being",
"they",
"them",
"than",
"then",
"also",
"some",
"more",
"only",
"over",
"such",
"very",
"through",
"anime",
"story",
"series",
];


text.to_lowercase()
    .split_whitespace()
    .map(|word| {
        word.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    })
    .filter(|word| {
        word.len() >= 4
            && !STOPWORDS.contains(&word.as_str())
    })
    .collect()


}

// ============================================================
// TASTE PROFILE
// ============================================================

struct TasteProfile {
genres: HashMap<String, f32>,
tags: HashMap<String, f32>,
keywords: HashMap<String, f32>,
}

fn build_taste_profile(history: &[Anime]) -> TasteProfile {
println!();
println!("🧠 BUILDING TASTE PROFILE");
println!();


let mut genres = HashMap::new();
let mut tags = HashMap::new();
let mut keywords = HashMap::new();

for anime in history {
    println!(
        "   🧠 Learning from: {}",
        get_display_title(anime)
    );

    for genre in &anime.genres {
        let key = normalize_title(genre);

        *genres.entry(key).or_insert(0.0) += 1.0;
    }

    for tag in &anime.tags {
        let key = normalize_title(&tag.name);

        if key.is_empty() {
            continue;
        }

        let weight = (tag.rank / 100.0).max(0.1);

        *tags.entry(key).or_insert(0.0) += weight;
    }

    if let Some(description) = &anime.description {
        for keyword in extract_keywords(description) {
            *keywords.entry(keyword).or_insert(0.0) += 1.0;
        }
    }
}

println!();

println!(
    "   🎭 Learned {} genre signals.",
    genres.len()
);

println!(
    "   🏷️ Learned {} tag signals.",
    tags.len()
);

println!(
    "   🔑 Learned {} keyword signals.",
    keywords.len()
);

TasteProfile {
    genres,
    tags,
    keywords,
}


}

// ============================================================
// SCORING
// ============================================================

fn genre_score(
anime: &Anime,
profile: &TasteProfile,
) -> f32 {
anime
.genres
.iter()
.map(|genre| {
let key = normalize_title(genre);


        profile
            .genres
            .get(&key)
            .copied()
            .unwrap_or(0.0)
    })
    .sum::<f32>()
    * 100.0


}

fn tag_score(
anime: &Anime,
profile: &TasteProfile,
) -> f32 {
anime
.tags
.iter()
.map(|tag| {
let key = normalize_title(&tag.name);


        let preference = profile
            .tags
            .get(&key)
            .copied()
            .unwrap_or(0.0);

        preference * (tag.rank / 100.0) * 100.0
    })
    .sum()


}

fn keyword_score(
anime: &Anime,
profile: &TasteProfile,
) -> f32 {
let Some(description) = &anime.description else {
return 0.0;
};


extract_keywords(description)
    .iter()
    .map(|keyword| {
        profile
            .keywords
            .get(keyword)
            .copied()
            .unwrap_or(0.0)
    })
    .sum::<f32>()
    * 10.0


}

fn graph_score(evidence: &CandidateEvidence) -> f32 {
let recommendation_score =
evidence.recommendation_sources.len() as f32 * 30.0;


let relation_score =
    evidence.relation_sources.len() as f32 * 15.0;

let deep_score =
    evidence.deep_sources.len() as f32 * 8.0;

recommendation_score + relation_score + deep_score


}

fn year_score(
anime: &Anime,
history: &[Anime],
) -> f32 {
let Some(year) = anime.start_year else {
return 0.0;
};


let history_years: Vec<i32> = history
    .iter()
    .filter_map(|anime| anime.start_year)
    .collect();

if history_years.is_empty() {
    return 0.0;
}

let average = history_years.iter().sum::<i32>() as f32
    / history_years.len() as f32;

let difference = (year as f32 - average).abs();

(100.0 - difference).max(-100.0)


}

// ============================================================
// EROTIC / UNSUITABLE FILTER
// ============================================================

fn is_erotic(anime: &Anime) -> bool {
let blocked = [
"hentai",
"porn",
"pornographic",
"erotica",
"erotic",
"ecchi",
];


let mut text = get_display_title(anime);

for tag in &anime.tags {
    text.push(' ');
    text.push_str(&tag.name);
}

if let Some(description) = &anime.description {
    text.push(' ');
    text.push_str(description);
}

let normalized = normalize_title(&text);

blocked.iter().any(|blocked_word| {
    normalized
        .split_whitespace()
        .any(|word| word == *blocked_word)
})


}

// ============================================================
// CANDIDATE SCORE
// ============================================================

#[derive(Debug)]
struct CandidateScore {
total: f32,
genre: f32,
tags: f32,
keywords: f32,
graph: f32,
year: f32,
}

fn score_candidate(
anime: &Anime,
evidence: &CandidateEvidence,
profile: &TasteProfile,
history: &[Anime],
) -> CandidateScore {
let genre = genre_score(anime, profile);


let tags = tag_score(anime, profile);

let keywords = keyword_score(anime, profile);

let graph = graph_score(evidence);

let year = year_score(anime, history);

let total =
    genre
    + tags
    + keywords
    + graph
    + year;

CandidateScore {
    total,
    genre,
    tags,
    keywords,
    graph,
    year,
}


}

// ============================================================
// DEEP CANDIDATE CHECKING
// ============================================================

fn check_all_candidates(
candidates: &HashMap<i32, CandidateEvidence>,
cache: &mut HashMap<i32, Anime>,
watched_ids: &HashSet<i32>,
owned_titles: &HashSet<String>,
history: &[Anime],
profile: &TasteProfile,
) -> Vec<(Anime, CandidateScore)> {
println!();
println!("════════════════════════════════════");
println!("🔬 DEEP CANDIDATE CHECK");
println!("════════════════════════════════════");
println!();


let mut ids: Vec<i32> =
    candidates.keys().copied().collect();

ids.sort_by(|left, right| {
    let left_strength = candidates
        .get(left)
        .map(CandidateEvidence::strength)
        .unwrap_or(0);

    let right_strength = candidates
        .get(right)
        .map(CandidateEvidence::strength)
        .unwrap_or(0);

    right_strength.cmp(&left_strength)
});

// --------------------------------------------------------
// Fetch metadata for EVERY candidate.
// --------------------------------------------------------

ensure_anime_loaded(&ids, cache);

println!();

println!(
    "🔎 Now checking {} candidates individually...",
    ids.len()
);

println!();

let mut results = Vec::new();
let mut checked = 0usize;
let mut rejected = 0usize;

for id in ids {
    checked += 1;

    println!(
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    );

    println!(
        "🔍 CHECKING ANIME {}/{}",
        checked,
        candidates.len()
    );

    let Some(evidence) = candidates.get(&id) else {
        continue;
    };

    println!("📌 Candidate: {}", evidence.title);
    println!("🆔 AniList ID: {}", id);

    let Some(anime) = cache.get(&id).cloned() else {
        println!("   ❌ Metadata could not be loaded.");
        rejected += 1;
        continue;
    };

    println!(
        "🎬 AniList title: {}",
        get_display_title(&anime)
    );

    // ----------------------------------------------------
    // WATCHED
    // ----------------------------------------------------

    if watched_ids.contains(&anime.id) {
        println!("   ❌ REJECTED: already in history.");
        rejected += 1;
        continue;
    }

    // ----------------------------------------------------
    // FIRST-SEASON RESOLUTION
    // ----------------------------------------------------

    let resolved = resolve_first_season(
        anime,
        cache,
        watched_ids,
        owned_titles,
    );

    let anime = match resolved {
        ResolutionResult::Recommend(anime) => anime,

        ResolutionResult::AlreadyOwned { title } => {
            println!(
                "   ❌ REJECTED: {} is already owned.",
                title
            );

            rejected += 1;
            continue;
        }

        ResolutionResult::Watched { title } => {
            println!(
                "   ❌ REJECTED: {} is already watched.",
                title
            );

            rejected += 1;
            continue;
        }

        ResolutionResult::Invalid => {
            println!(
                "   ❌ REJECTED: could not resolve franchise root."
            );

            rejected += 1;
            continue;
        }
    };

    // ----------------------------------------------------
    // OWNERSHIP AFTER FIRST-SEASON RESOLUTION
    // ----------------------------------------------------

    if anime_matches_owned_title(
        &anime,
        owned_titles,
    ) {
        println!(
            "   ❌ REJECTED: first/main season is already owned."
        );

        rejected += 1;
        continue;
    }

    if watched_ids.contains(&anime.id) {
        println!(
            "   ❌ REJECTED: first/main season is already watched."
        );

        rejected += 1;
        continue;
    }

    // ----------------------------------------------------
    // FORMAT
    // ----------------------------------------------------

    match anime.anime_type.as_deref() {
        Some("TV") | Some("TV_SHORT") => {}

        Some(other) => {
            println!(
                "   ❌ REJECTED: format is {}.",
                other
            );

            rejected += 1;
            continue;
        }

        None => {
            println!(
                "   ❌ REJECTED: unknown format."
            );

            rejected += 1;
            continue;
        }
    }

    // ----------------------------------------------------
    // EROTIC FILTER
    // ----------------------------------------------------

    if is_erotic(&anime) {
        println!(
            "   ❌ REJECTED: filtered content."
        );

        rejected += 1;
        continue;
    }

    // ----------------------------------------------------
    // SCORE
    // ----------------------------------------------------

    let score = score_candidate(
        &anime,
        candidates.get(&id).unwrap(),
        profile,
        history,
    );

    println!("   ✅ PASSED FILTERS");

    println!(
        "   🎬 FINAL ENTRY: {}",
        get_display_title(&anime)
    );

    println!(
        "   ⭐ Score: {:.2}",
        score.total
    );

    println!(
        "      🎭 Genre: {:.2}",
        score.genre
    );

    println!(
        "      🏷️ Tags: {:.2}",
        score.tags
    );

    println!(
        "      🔑 Keywords: {:.2}",
        score.keywords
    );

    println!(
        "      🧬 Graph: {:.2}",
        score.graph
    );

    println!(
        "      📅 Year: {:.2}",
        score.year
    );

    results.push((anime, score));
}

println!();

println!("════════════════════════════════════");
println!("🔬 CANDIDATE CHECK COMPLETE");

println!("   Checked: {}", checked);
println!("   Rejected: {}", rejected);
println!("   Passed: {}", results.len());

println!("════════════════════════════════════");

results


}

// ============================================================
// FINAL DEDUPLICATION
// ============================================================

fn deduplicate_results(
results: Vec<(Anime, CandidateScore)>,
) -> Vec<(Anime, CandidateScore)> {
let mut seen = HashSet::new();
let mut output = Vec::new();


for result in results {
    let key = normalize_title(
        &get_display_title(&result.0),
    );

    if seen.insert(key) {
        output.push(result);
    }
}

output


}

// ============================================================
// PUBLIC RECOMMENDATION API
// ============================================================

pub fn get_recommendations() -> Vec<String> {
println!();
println!("🧠 ORION PREDICTION ENGINE");
println!("════════════════════════════════════");
println!();


// --------------------------------------------------------
// LOCAL LIBRARY
// --------------------------------------------------------

let owned_titles = read_owned_titles();

// --------------------------------------------------------
// ANI-CLI HISTORY
// --------------------------------------------------------

let history_titles = read_ani_cli_history();

if history_titles.is_empty() {
    println!("❌ No Ani-CLI watch history or liked anime found.");
    return Vec::new();
}

println!(
    "📊 Prediction taste inputs: {} anime",
    history_titles.len()
);

// --------------------------------------------------------
// CACHE
// --------------------------------------------------------

let mut cache: HashMap<i32, Anime> = HashMap::new();

// --------------------------------------------------------
// RESOLVE HISTORY
// --------------------------------------------------------

let history = resolve_history(
    &history_titles,
    &mut cache,
);

if history.is_empty() {
    println!(
        "❌ No history entries could be resolved."
    );

    return Vec::new();
}

// --------------------------------------------------------
// WATCHED IDS
// --------------------------------------------------------

let watched_ids: HashSet<i32> =
    history.iter().map(|anime| anime.id).collect();

println!();
println!("👁️ Resolved history:");

for anime in &history {
    println!(
        "   • {} → {}",
        get_display_title(anime),
        anime.id
    );
}

// --------------------------------------------------------
// TASTE
// --------------------------------------------------------

let profile = build_taste_profile(&history);

// --------------------------------------------------------
// FIRST HOP
// --------------------------------------------------------

let mut candidates = build_first_hop_candidates(
    &history,
    &watched_ids,
);

// Keep the initial pool bounded.
//
// Stronger candidates survive.
if candidates.len() > MAX_FIRST_HOP_CANDIDATES {
    let mut ranked: Vec<_> =
        candidates.into_iter().collect();

    ranked.sort_by(|left, right| {
        right
            .1
            .strength()
            .cmp(&left.1.strength())
    });

    ranked.truncate(MAX_FIRST_HOP_CANDIDATES);

    candidates =
        ranked.into_iter().collect();
}

println!();
println!(
    "🎯 First-hop pool limited to {} candidates.",
    candidates.len()
);

// --------------------------------------------------------
// LOAD FIRST-HOP METADATA
// --------------------------------------------------------

let first_hop_ids: Vec<i32> =
    candidates.keys().copied().collect();

ensure_anime_loaded(
    &first_hop_ids,
    &mut cache,
);

// --------------------------------------------------------
// SECOND HOP
// --------------------------------------------------------

build_second_hop_candidates(
    &mut candidates,
    &mut cache,
    &watched_ids,
);

// --------------------------------------------------------
// DEEP CHECK
// --------------------------------------------------------

let mut results = check_all_candidates(
    &candidates,
    &mut cache,
    &watched_ids,
    &owned_titles,
    &history,
    &profile,
);

// --------------------------------------------------------
// SORT
// --------------------------------------------------------

results.sort_by(|left, right| {
    right
        .1
        .total
        .partial_cmp(&left.1.total)
        .unwrap_or(std::cmp::Ordering::Equal)
});

// --------------------------------------------------------
// FRANCHISE DEDUPLICATION
// --------------------------------------------------------

let results = deduplicate_results(results);

// --------------------------------------------------------
// FINAL OUTPUT
// --------------------------------------------------------

println!();
println!("════════════════════════════════════");
println!("🎯 ORION FINAL OUTPUT");
println!("════════════════════════════════════");

if results.is_empty() {
    println!("❌ No new valid anime found.");
    return Vec::new();
}

let final_results: Vec<String> = results
    .iter()
    .take(MAX_RESULTS)
    .map(|(anime, _)| get_display_title(anime))
    .collect();

for (index, title) in final_results.iter().enumerate() {
    println!("{}. {}", index + 1, title);
}

println!();

println!(
    "🧠 Orion searched deeply through {} candidates.",
    candidates.len()
);

println!(
    "📚 {} local anime were excluded from recommendations.",
    owned_titles.len()
);

println!(
    "👁️ {} history entries were used as taste data.",
    history.len()
);

final_results


}

// ============================================================
// MAIN
// ============================================================

fn main() {
let recommendations = get_recommendations();


println!();
println!("════════════════════════════════════");
println!("🏁 PREDICTION TEST FINISHED");
println!("════════════════════════════════════");

if recommendations.is_empty() {
    println!("No recommendations generated.");
    return;
}

println!();

for (index, recommendation) in recommendations.iter().enumerate() {
    println!(
        "{}. {}",
        index + 1,
        recommendation
    );
}


}
