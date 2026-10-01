import argparse
import os
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import urljoin

import requests


# ============================================================
# CONFIGURATION
# ============================================================

USER_AGENT = (
    "Mozilla/5.0 (X11; Linux x86_64) "
    "AppleWebKit/537.36 (KHTML, like Gecko) "
    "Chrome/154.0.0.0 Safari/537.36"
)

REQUEST_TIMEOUT = 15
SEGMENT_TEST_COUNT = 3


# ============================================================
# MIRURO BACKENDS
# ============================================================

MIRURO_BACKENDS = [
    {
        "name": "Miruro Native API",
        "base": "https://api.hiyori.tv",
        "episodes_path": "/episodes/{anilist_id}",
        "watch_path": "/watch/{provider}/{episode_id}",
    },
    {
        "name": "miruro.to",
        "base": "https://miruro.to",
        "episodes_path": "/api/episodes/{anilist_id}",
        "watch_path": "/api/watch/{provider}/{episode_id}",
    },
    {
        "name": "miruro.bz",
        "base": "https://miruro.bz",
        "episodes_path": "/api/episodes/{anilist_id}",
        "watch_path": "/api/watch/{provider}/{episode_id}",
    },
    {
        "name": "miruro.tv",
        "base": "https://miruro.tv",
        "episodes_path": "/api/episodes/{anilist_id}",
        "watch_path": "/api/watch/{provider}/{episode_id}",
    },
    {
        "name": "miruro.cx",
        "base": "https://miruro.cx",
        "episodes_path": "/api/episodes/{anilist_id}",
        "watch_path": "/api/watch/{provider}/{episode_id}",
    },
]

ACTIVE_MIRURO_BACKEND = None

# ============================================================
# SESSION
# ============================================================

SESSION = requests.Session()

SESSION.headers.update(
    {
        "User-Agent": USER_AGENT,
        "Accept": "*/*",
        "Accept-Language": "en-US,en;q=0.9",
    }
)


# ============================================================
# GENERAL HELPERS
# ============================================================

def safe_filename(value):
    """
    Convert arbitrary text into a filesystem-safe filename.
    """
    value = str(value)

    value = re.sub(
        r"[^A-Za-z0-9._-]+",
        "_",
        value,
    )

    value = value.strip("._ ")

    return value or "unknown"


def request_headers(origin=None):
    """
    Headers used when communicating with a Miruro website/API.
    """
    headers = {
        "User-Agent": USER_AGENT,
        "Accept": "*/*",
        "Accept-Language": "en-US,en;q=0.9",
    }

    if origin:
        origin = origin.rstrip("/")

        headers.update(
            {
                "Referer": origin + "/",
                "Origin": origin,
            }
        )

    return headers



# ANILIST


def search_anilist(title):
    query = """
    query ($search: String) {
        Media(search: $search, type: ANIME) {
            id
            title {
                romaji
                english
            }
            episodes
        }
    }
    """

    try:
        response = SESSION.post(
            "https://graphql.anilist.co",
            json={
                "query": query,
                "variables": {
                    "search": title,
                },
            },
            timeout=REQUEST_TIMEOUT,
        )

        if response.status_code != 200:
            print(
                f"[-] AniList HTTP {response.status_code}"
            )
            return None

        return (
            response.json()
            .get("data", {})
            .get("Media")
        )

    except requests.RequestException as exc:
        print(
            f"[-] AniList request failed: {exc}"
        )
        return None

    except ValueError:
        print(
            "[-] AniList returned invalid JSON."
        )
        return None



# MIRURO BACKEND URL BUILDING


def build_episode_url(backend, anilist_id):
    return (
        backend["base"]
        + backend["episodes_path"].format(
            anilist_id=anilist_id
        )
    )


def build_watch_url(
    backend,
    provider_name,
    episode_id,
):
    clean_id = str(episode_id).lstrip("/")

    # Some APIs return the complete watch path.
    if clean_id.startswith("watch/"):
        if backend["name"] == "Miruro Native API":
            return (
                f"{backend['base']}/{clean_id}"
            )

        return (
            f"{backend['base']}/api/{clean_id}"
        )

    return (
        backend["base"]
        + backend["watch_path"].format(
            provider=provider_name,
            episode_id=clean_id,
        )
    )



# MIRURO BACKEND HEALTH / ROTATION


def backend_label(backend):
    return backend["name"]


def ordered_backends():
    """
    Put the last successful backend first.

    If it fails, every other configured backend
    is still attempted.
    """
    if ACTIVE_MIRURO_BACKEND is None:
        return list(MIRURO_BACKENDS)

    preferred = []

    for backend in MIRURO_BACKENDS:
        if (
            backend["base"]
            == ACTIVE_MIRURO_BACKEND["base"]
        ):
            preferred.insert(0, backend)
        else:
            preferred.append(backend)

    return preferred


def mark_backend_success(backend):
    global ACTIVE_MIRURO_BACKEND

    ACTIVE_MIRURO_BACKEND = backend



# MIRURO PROVIDER DISCOVERY


def get_provider_candidates(
    anilist_id,
    episode_num,
):
    """
    Discover providers for one episode.

    We try every configured Miruro backend until
    one provides usable provider information.
    """

    print(
        "\n"
        + "=" * 60
    )

    print(
        "[+] MIRURO BACKEND FAILOVER"
    )

    print(
        "=" * 60
    )

    for backend in ordered_backends():
        name = backend_label(backend)

        url = build_episode_url(
            backend,
            anilist_id,
        )

        print(
            f"\n[*] Trying {name}"
        )

        print(
            f"    {url}"
        )

        try:
            response = SESSION.get(
                url,
                headers=request_headers(
                    backend["base"]
                ),
                timeout=REQUEST_TIMEOUT,
            )

        except requests.RequestException as exc:
            print(
                f"    [-] Connection failed: {exc}"
            )
            continue

        print(
            f"    HTTP {response.status_code}"
        )

        if response.status_code != 200:
            print(
                f"    [-] {name} unavailable."
            )
            continue

        try:
            data = response.json()

        except ValueError:
            print(
                f"    [-] {name} returned invalid JSON."
            )
            continue

        if (
            isinstance(data, dict)
            and isinstance(
                data.get("results"),
                dict,
            )
        ):
            data = data["results"]

        if not isinstance(data, dict):
            print(
                "    [-] Invalid backend response."
            )
            continue

        providers = data.get(
            "providers",
            {},
        )

        if not isinstance(providers, dict):
            print(
                "    [-] No usable provider structure."
            )
            continue

        candidates = []

        for provider_name, provider_data in providers.items():
            if not isinstance(
                provider_data,
                dict,
            ):
                continue

            episodes = provider_data.get(
                "episodes",
                {},
            )

            if not isinstance(
                episodes,
                dict,
            ):
                continue

            sub_episodes = episodes.get(
                "sub",
                [],
            )

            if not isinstance(
                sub_episodes,
                list,
            ):
                continue

            for episode in sub_episodes:
                if not isinstance(
                    episode,
                    dict,
                ):
                    continue

                number = episode.get(
                    "number"
                )

                if number != episode_num:
                    continue

                episode_id = episode.get(
                    "id"
                )

                if not episode_id:
                    continue

                candidates.append(
                    {
                        "provider": provider_name,
                        "episode_id": episode_id,
                        "backend": backend,
                    }
                )

                print(
                    f"    [+] {provider_name}: "
                    f"episode {episode_num} "
                    f"-> {episode_id}"
                )

        if candidates:
            print(
                f"\n[+] {name} supplied "
                f"{len(candidates)} "
                f"provider candidate(s)."
            )

            mark_backend_success(
                backend
            )

            return candidates

        print(
            f"    [-] {name} has no provider "
            f"data for episode {episode_num}."
        )

    print(
        "\n[-] ALL MIRURO BACKENDS FAILED "
        "TO PROVIDE EPISODE DATA."
    )

    return []



# GET STREAMS FROM ONE PROVIDER


def fetch_provider_streams(
    provider_name,
    episode_id,
    backend,
):
    """
    Request actual streams from one provider.
    """

    watch_url = build_watch_url(
        backend,
        provider_name,
        episode_id,
    )

    print(
        f"\n[*] Trying provider "
        f"'{provider_name}'..."
    )

    print(
        f"[*] Backend: "
        f"{backend_label(backend)}"
    )

    print(
        f"[*] Requesting: {watch_url}"
    )

    try:
        response = SESSION.get(
            watch_url,
            headers=request_headers(
                backend["base"]
            ),
            timeout=REQUEST_TIMEOUT,
        )

    except requests.RequestException as exc:
        print(
            f"    [-] Request failed: {exc}"
        )
        return None

    if response.status_code != 200:
        print(
            f"    [-] HTTP "
            f"{response.status_code}"
        )
        return None

    try:
        stream_data = response.json()

    except ValueError:
        print(
            "    [-] Provider returned invalid JSON."
        )
        return None

    if (
        isinstance(stream_data, dict)
        and isinstance(
            stream_data.get("results"),
            dict,
        )
    ):
        stream_data = stream_data["results"]

    if not isinstance(stream_data, dict):
        print(
            "    [-] Invalid provider response."
        )
        return None

    streams = stream_data.get(
        "streams",
        [],
    )

    if not isinstance(
        streams,
        list,
    ):
        print(
            "    [-] Invalid stream list."
        )
        return None

    hls_streams = []

    for index, stream in enumerate(streams):
        if not isinstance(
            stream,
            dict,
        ):
            continue

        print(
            f"    [stream {index}] "
            f"type={stream.get('type')} "
            f"quality={stream.get('quality')}"
        )

        if stream.get("type") != "hls":
            continue

        url = stream.get("url")

        if not url:
            continue

        hls_streams.append(
            stream
        )

    if not hls_streams:
        print(
            f"    [-] No HLS streams "
            f"from '{provider_name}'."
        )
        return None

    subtitles = stream_data.get(
        "subtitles",
        [],
    )

    if not isinstance(
        subtitles,
        list,
    ):
        subtitles = []

    print(
        f"    [+] Found "
        f"{len(hls_streams)} HLS streams"
    )

    print(
        f"    [+] Found "
        f"{len(subtitles)} subtitle track(s)"
    )

    for index, stream in enumerate(hls_streams):
        print(
            f"    [HLS {index}] "
            f"{stream.get('url')}"
        )

        print(
            f"        server="
            f"{stream.get('server')}"
        )

        print(
            f"        referer="
            f"{stream.get('referer')}"
        )

        print(
            f"        priority="
            f"{stream.get('priority')}"
        )

        print(
            f"        isActive="
            f"{stream.get('isActive')}"
        )

    return {
        "provider": provider_name,
        "episode_id": str(
            episode_id
        ).lstrip("/"),
        "backend": backend,
        "streams": hls_streams,
        "subtitles": subtitles,
        "intro": stream_data.get("intro"),
        "outro": stream_data.get("outro"),
    }



# GET ALL PROVIDER STREAM DATA


def get_stream_data(
    anilist_id,
    episode_num,
):
    """
    Discover providers and ask every discovered
    provider for its streams.
    """

    candidates = get_provider_candidates(
        anilist_id,
        episode_num,
    )

    if not candidates:
        return []

    results = []

    for candidate in candidates:
        provider_name = candidate["provider"]
        episode_id = candidate["episode_id"]
        backend = candidate["backend"]

        stream_info = fetch_provider_streams(
            provider_name,
            episode_id,
            backend,
        )

        if not stream_info:
            continue

        results.append(
            stream_info
        )

    return results



# HTTP STREAM HELPERS


def build_headers(referer):
    """
    Build headers required by the actual stream server.
    """

    if not referer:
        return None

    if not referer.endswith("/"):
        referer += "/"

    origin = referer.rstrip("/")

    return {
        "User-Agent": USER_AGENT,
        "Referer": referer,
        "Origin": origin,
        "Accept": "*/*",
    }


def fetch_text(url, headers):
    """
    Fetch an HLS playlist.
    """

    try:
        response = SESSION.get(
            url,
            headers=headers,
            timeout=REQUEST_TIMEOUT,
        )

        print(
            f"    HTTP {response.status_code}"
        )

        if response.status_code != 200:
            return None

        return response.text

    except requests.RequestException as exc:
        print(
            f"    [-] Request failed: {exc}"
        )
        return None



# PLAYLIST PARSING


def find_child_playlists(
    master_url,
    master_playlist,
):
    playlists = []

    for line in master_playlist.splitlines():
        line = line.strip()

        if not line:
            continue

        if line.startswith("#"):
            continue

        if ".m3u8" not in line:
            continue

        child_url = urljoin(
            master_url,
            line,
        )

        if child_url not in playlists:
            playlists.append(
                child_url
            )

    return playlists


def find_media_segments(
    playlist_url,
    playlist,
):
    segments = []

    for line in playlist.splitlines():
        line = line.strip()

        if not line:
            continue

        if line.startswith("#"):
            continue

        if ".m3u8" in line:
            continue

        segments.append(
            urljoin(
                playlist_url,
                line,
            )
        )

    return segments


# ============================================================
# MEDIA SIGNATURE CHECKING
# ============================================================

def looks_like_media(
    sample,
    content_type,
):
    """
    Determine whether returned bytes plausibly
    represent media rather than HTML/JavaScript.

    We intentionally accept unknown binary data because
    some providers use misleading MIME types and filenames.
    """

    if not sample:
        return False

    lowered_type = (
        content_type or ""
    ).lower()

    forbidden_types = (
        "text/html",
        "text/plain",
        "text/css",
        "javascript",
        "application/javascript",
        "application/json",
        "application/xml",
        "text/xml",
    )

    if any(
        bad in lowered_type
        for bad in forbidden_types
    ):
        return False

    # Common MPEG-TS signature.
    if len(sample) >= 1 and sample[0] == 0x47:
        return True

    # MP4 / fragmented MP4 / ISO-BMFF.
    if len(sample) >= 8:
        if sample[4:8] == b"ftyp":
            return True

        if sample[4:8] == b"moof":
            return True

        if sample[4:8] == b"styp":
            return True

    # Matroska/WebM EBML.
    if sample.startswith(
        b"\x1A\x45\xDF\xA3"
    ):
        return True

    # JPEG is accepted because some of these HLS
    # providers deliberately disguise binary media
    # segments with .jpg URLs.
    if sample.startswith(
        b"\xFF\xD8\xFF"
    ):
        return True

    # If the MIME type explicitly identifies binary
    # media, accept it.
    binary_types = (
        "video/",
        "audio/",
        "application/octet-stream",
        "application/mp4",
        "application/vnd.apple.mpegurl",
    )

    if any(
        media_type in lowered_type
        for media_type in binary_types
    ):
        return True

    # Unknown binary content.
    try:
        decoded = sample.decode(
            "utf-8",
            errors="strict",
        )
    except UnicodeDecodeError:
        return True

    # If it successfully decoded as UTF-8 and contains
    # obvious source-code/HTML markers, reject it.
    lowered = decoded.lower()

    suspicious_markers = (
        "<html",
        "<!doctype",
        "<script",
        "function(",
        "function ",
        "const ",
        "let ",
        "var ",
        "window.",
        "document.",
        "body {",
        "{\n",
    )

    if any(
        marker in lowered
        for marker in suspicious_markers
    ):
        return False

    return False



# TEST ACTUAL MEDIA SEGMENTS


def test_media_segments(
    segments,
    headers,
):
    if not segments:
        print(
            "    [-] No media segments found."
        )
        return False

    test_count = min(
        SEGMENT_TEST_COUNT,
        len(segments),
    )

    print(
        f"\n[+] Found {len(segments)} "
        f"media segments."
    )

    print(
        f"[*] Testing first "
        f"{test_count} segment(s)..."
    )

    working = 0

    for number, segment_url in enumerate(
        segments[:test_count],
        start=1,
    ):
        print(
            f"\n[*] Testing segment "
            f"{number}/{test_count}"
        )

        print(
            f"    {segment_url}"
        )

        segment_headers = dict(
            headers
        )

        segment_headers["Range"] = (
            "bytes=0-1023"
        )

        try:
            response = SESSION.get(
                segment_url,
                headers=segment_headers,
                timeout=REQUEST_TIMEOUT,
                stream=True,
            )

            status = response.status_code

            content_type = response.headers.get(
                "Content-Type",
                "unknown",
            )

            content_length = response.headers.get(
                "Content-Length",
                "unknown",
            )

            print(
                f"    HTTP {status}"
            )

            print(
                f"    Content-Type: "
                f"{content_type}"
            )

            print(
                f"    Content-Length: "
                f"{content_length}"
            )

            sample = next(
                response.iter_content(
                    chunk_size=1024
                ),
                b"",
            )

            response.close()

            print(
                f"    Sample bytes: "
                f"{len(sample)}"
            )

            if status not in (
                200,
                206,
            ):
                print(
                    "    [-] Segment unavailable."
                )
                continue

            if not sample:
                print(
                    "    [-] Empty response."
                )
                continue

            if not looks_like_media(
                sample,
                content_type,
            ):
                print(
                    "    [-] Response does not "
                    "look like media."
                )
                continue

            print(
                "    [+] MEDIA SEGMENT WORKS!"
            )

            working += 1

        except requests.RequestException as exc:
            print(
                f"    [-] Segment request failed: "
                f"{exc}"
            )

    print(
        f"\n[*] Working media segments: "
        f"{working}/{test_count}"
    )

    return working > 0



# VALIDATE ONE HLS SOURCE


def validate_hls_stream(stream):
    """
    Verify:

        master playlist
             ↓
        child playlist
             ↓
        real media segment
    """

    url = stream.get("url")

    if not url:
        return False

    referer = stream.get("referer")

    if not referer:
        print(
            "    [-] HLS source has no referer."
        )
        return False

    headers = build_headers(
        referer
    )

    if not headers:
        return False

    print(
        "\n[*] Testing HLS source..."
    )

    print(
        f"    Server: "
        f"{stream.get('server')}"
    )

    print(
        f"    URL: {url}"
    )

    print(
        f"    Referer: "
        f"{headers['Referer']}"
    )

    print(
        f"    Origin: "
        f"{headers['Origin']}"
    )

    print(
        f"    Priority: "
        f"{stream.get('priority')}"
    )

    print(
        f"    Active: "
        f"{stream.get('isActive')}"
    )

    print(
        "\n[*] Fetching master playlist..."
    )

    master_text = fetch_text(
        url,
        headers,
    )

    if not master_text:
        print(
            "    [-] Master playlist failed."
        )
        return False

    print(
        f"    [+] Master playlist received "
        f"({len(master_text.encode())} bytes)"
    )

    child_playlists = find_child_playlists(
        url,
        master_text,
    )

    if not child_playlists:
        print(
            "    [-] No child playlist found."
        )
        return False

    print(
        f"\n[+] Found "
        f"{len(child_playlists)} "
        f"child playlist(s)."
    )

    for child_url in child_playlists:
        print(
            f"\n[*] Child playlist:\n"
            f"    {child_url}"
        )

        child_text = fetch_text(
            child_url,
            headers,
        )

        if not child_text:
            print(
                "    [-] Child playlist failed."
            )
            continue

        print(
            f"    [+] Child playlist received "
            f"({len(child_text.encode())} bytes)"
        )

        segments = find_media_segments(
            child_url,
            child_text,
        )

        if not segments:
            print(
                "    [-] No media segments "
                "in this playlist."
            )
            continue

        if test_media_segments(
            segments,
            headers,
        ):
            print(
                "\n    [+] HLS source "
                "PASSED validation."
            )
            return True

        print(
            "\n    [-] HLS source "
            "FAILED media validation."
        )

    return False



# SUBTITLE HELPERS


def get_subtitle_url(
    subtitle,
    base_url=None,
):
    if isinstance(
        subtitle,
        str,
    ):
        url = subtitle

    elif isinstance(
        subtitle,
        dict,
    ):
        url = (
            subtitle.get("url")
            or subtitle.get("file")
            or subtitle.get("src")
            or subtitle.get("source")
        )

    else:
        return None

    if not url:
        return None

    if base_url:
        url = urljoin(
            base_url,
            url,
        )

    return url


def get_subtitle_label(
    subtitle,
    index,
):
    if isinstance(
        subtitle,
        dict,
    ):
        label = (
            subtitle.get("label")
            or subtitle.get("language")
            or subtitle.get("lang")
            or subtitle.get("name")
        )

        if label:
            return str(label)

    return f"Subtitle {index}"


def download_subtitle(
    subtitle,
    output_dir,
    episode_number,
    index,
    headers,
):
    url = get_subtitle_url(
        subtitle
    )

    if not url:
        print(
            "    [-] Subtitle has no usable URL."
        )
        return None

    label = get_subtitle_label(
        subtitle,
        index,
    )

    print(
        f"    [*] Downloading subtitle: "
        f"{label}"
    )

    try:
        response = SESSION.get(
            url,
            headers=headers,
            timeout=REQUEST_TIMEOUT,
        )

        if response.status_code != 200:
            print(
                f"    [-] Subtitle HTTP "
                f"{response.status_code}"
            )
            return None

        if not response.content:
            print(
                "    [-] Subtitle is empty."
            )
            return None

        lowered_url = url.lower()

        if ".ass" in lowered_url:
            extension = ".ass"
        elif ".ssa" in lowered_url:
            extension = ".ssa"
        elif ".vtt" in lowered_url:
            extension = ".vtt"
        else:
            extension = ".srt"

        safe_label = safe_filename(
            label
        )

        output_dir.mkdir(
            parents=True,
            exist_ok=True,
        )

        subtitle_path = (
            output_dir
            / (
                f"episode_{episode_number:03d}."
                f"{safe_label}"
                f"{extension}"
            )
        )

        subtitle_path.write_bytes(
            response.content
        )

        print(
            f"    [+] Subtitle saved: "
            f"{subtitle_path}"
        )

        return subtitle_path

    except requests.RequestException as exc:
        print(
            f"    [-] Subtitle download failed: "
            f"{exc}"
        )
        return None



# FFMPEG


def ffmpeg_available():
    try:
        result = subprocess.run(
            [
                "ffmpeg",
                "-version",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )

        return result.returncode == 0

    except FileNotFoundError:
        return False


def get_episode_output_path(
    output_dir,
    anime_title,
    episode_number,
):
    safe_title = safe_filename(
        anime_title
    )

    return (
        output_dir
        / (
            f"{safe_title} - "
            f"E{episode_number:03d}.mkv"
        )
    )


def download_video(
    stream,
    subtitle_paths,
    anime_title,
    episode_number,
    output_dir,
):
    if not ffmpeg_available():
        print(
            "[-] FFmpeg is not installed."
        )

        print(
            "[-] Install FFmpeg before "
            "using download mode."
        )

        return False

    url = stream.get("url")
    referer = stream.get("referer")

    if not url or not referer:
        print(
            "[-] Missing HLS URL or referer."
        )
        return False

    if not referer.endswith("/"):
        referer += "/"

    origin = referer.rstrip("/")

    output_dir.mkdir(
        parents=True,
        exist_ok=True,
    )

    output_path = get_episode_output_path(
        output_dir,
        anime_title,
        episode_number,
    )

    print(
        "\n"
        + "=" * 60
    )

    print(
        "[+] DOWNLOADING EPISODE"
    )

    print(
        "=" * 60
    )

    print(
        f"[*] Episode: {episode_number}"
    )

    print(
        f"[*] Provider: "
        f"{stream.get('provider')}"
    )

    print(
        f"[*] Backend: "
        f"{stream.get('backend_name')}"
    )

    print(
        f"[*] Server: "
        f"{stream.get('server')}"
    )

    print(
        f"[*] Output: "
        f"{output_path}"
    )

    headers = (
        f"Referer: {referer}\r\n"
        f"Origin: {origin}\r\n"
    )

    temp_video = output_path

    if subtitle_paths:
        temp_video = (
            output_dir
            / (
                f".{output_path.stem}"
                ".video.mkv"
            )
        )

    # IMPORTANT:
    #
    # These providers sometimes disguise HLS media
    # segments as .jpg/.html/.js/etc.
    #
    # FFmpeg's HLS demuxer normally rejects those
    # extensions. -allowed_extensions ALL tells FFmpeg
    # to accept the URLs from the playlist.
    #
    command = [
        "ffmpeg",
        "-y",

        "-user_agent",
        USER_AGENT,

        "-headers",
        headers,

        "-allowed_extensions",
        "ALL",

        "-i",
        url,

        "-map",
        "0:v:0",

        "-map",
        "0:a?",

        "-c",
        "copy",

        str(temp_video),
    ]

    print(
        "\n[*] Running FFmpeg..."
    )

    print(
        "[*] HLS -> MKV"
    )

    try:
        result = subprocess.run(
            command,
            check=False,
        )

    except FileNotFoundError:
        print(
            "[-] FFmpeg was not found."
        )
        return False

    if result.returncode != 0:
        print(
            "\n[-] FFmpeg failed."
        )

        if (
            temp_video != output_path
            and temp_video.exists()
        ):
            temp_video.unlink()

        return False

    
    # MUX SUBTITLES
    

    if subtitle_paths:
        print(
            "\n[+] Adding subtitles..."
        )

        mux_command = [
            "ffmpeg",
            "-y",
            "-i",
            str(temp_video),
        ]

        for subtitle_path in subtitle_paths:
            mux_command.extend(
                [
                    "-i",
                    str(subtitle_path),
                ]
            )

        # Keep all video/audio streams.
        mux_command.extend(
            [
                "-map",
                "0",
                "-c",
                "copy",
            ]
        )

        # Add every subtitle input.
        for index in range(
            len(subtitle_paths)
        ):
            mux_command.extend(
                [
                    "-map",
                    f"{index + 1}:0",
                ]
            )

        mux_command.append(
            str(output_path)
        )

        mux_result = subprocess.run(
            mux_command,
            check=False,
        )

        if mux_result.returncode != 0:
            print(
                "[-] Subtitle muxing failed."
            )

            print(
                "[*] Keeping the downloaded "
                "video instead."
            )

            if (
                temp_video.exists()
                and temp_video != output_path
            ):
                if output_path.exists():
                    output_path.unlink()

                temp_video.rename(
                    output_path
                )

        else:
            if (
                temp_video.exists()
                and temp_video != output_path
            ):
                temp_video.unlink()

    else:
        if (
            temp_video.exists()
            and temp_video != output_path
        ):
            temp_video.rename(
                output_path
            )

    
    # CLEAN TEMP SUBTITLES
    

    for subtitle_path in subtitle_paths:
        try:
            subtitle_path.unlink()
        except OSError:
            pass

    print(
        "\n"
        + "=" * 60
    )

    print(
        "[+] DOWNLOAD COMPLETE"
    )

    print(
        f"[+] {output_path}"
    )

    print(
        "=" * 60
    )

    return True



# MPV


def launch_mpv(
    stream,
    subtitles=None,
):
    url = stream["url"]

    referer = stream.get(
        "referer"
    )

    if not referer:
        print(
            "[-] Cannot start mpv: "
            "missing referer."
        )
        return False

    if not referer.endswith("/"):
        referer += "/"

    origin = referer.rstrip("/")

    header_string = (
        f"User-Agent: {USER_AGENT},"
        f"Referer: {referer},"
        f"Origin: {origin}"
    )

    cmd = [
        "mpv",
        "--ytdl=no",
        f"--user-agent={USER_AGENT}",
        f"--referrer={referer}",
        f"--http-header-fields={header_string}",
        "--cache=yes",
    ]

    subtitle_urls = []

    for subtitle in subtitles or []:
        subtitle_url = get_subtitle_url(
            subtitle
        )

        if subtitle_url:
            subtitle_urls.append(
                subtitle_url
            )

    if subtitle_urls:
        print(
            f"[+] Found "
            f"{len(subtitle_urls)} "
            f"subtitle track(s)."
        )

        cmd.append(
            f"--sub-file={subtitle_urls[0]}"
        )

    else:
        print(
            "[*] No external subtitle "
            "tracks were returned."
        )

    cmd.append(url)

    print(
        "\n[+] Starting mpv..."
    )

    print(
        f"    Provider: "
        f"{stream.get('provider')}"
    )

    print(
        f"    Backend: "
        f"{stream.get('backend_name')}"
    )

    print(
        f"    Server: "
        f"{stream.get('server')}"
    )

    try:
        subprocess.run(
            cmd,
            check=False,
        )

    except KeyboardInterrupt:
        print(
            "\n[*] Playback stopped."
        )

    return True



# STREAM SORTING


def stream_sort_key(stream):
    active = stream.get(
        "isActive",
        False,
    )

    priority = stream.get(
        "priority"
    )

    if not isinstance(
        priority,
        (int, float),
    ):
        priority = 999999

    return (
        not active,
        priority,
    )


def sort_streams_by_quality(
    streams,
    preferred_quality=None,
):
    streams = sorted(
        streams,
        key=stream_sort_key,
    )

    if not preferred_quality:
        return streams

    preferred_quality = (
        preferred_quality.lower()
    )

    def quality_key(stream):
        quality = str(
            stream.get(
                "quality",
                "",
            )
        ).lower()

        exact = (
            quality == preferred_quality
        )

        contains = (
            preferred_quality in quality
        )

        return (
            not exact,
            not contains,
        )

    return sorted(
        streams,
        key=quality_key,
    )



# PLAY ONE PROVIDER


def play_in_mpv(
    stream_info,
):
    provider = stream_info[
        "provider"
    ]

    episode_id = stream_info[
        "episode_id"
    ]

    backend = stream_info[
        "backend"
    ]

    streams = sort_streams_by_quality(
        stream_info["streams"]
    )

    subtitles = stream_info.get(
        "subtitles",
        [],
    )

    print(
        "\n[+] HLS sources found!"
    )

    print(
        f"[*] Provider: {provider}"
    )

    print(
        f"[*] Episode ID: {episode_id}"
    )

    print(
        f"[*] Backend: "
        f"{backend_label(backend)}"
    )

    tested_urls = set()

    for stream in streams:
        url = stream.get("url")

        if not url:
            continue

        if url in tested_urls:
            print(
                "\n[*] Skipping duplicate "
                "HLS URL."
            )
            continue

        tested_urls.add(url)

        if validate_hls_stream(
            stream
        ):
            print(
                "\n"
                + "=" * 60
            )

            print(
                "[+] PLAYABLE HLS SOURCE FOUND!"
            )

            print(
                "=" * 60
            )

            selected_stream = dict(
                stream
            )

            selected_stream["provider"] = (
                provider
            )

            selected_stream["backend_name"] = (
                backend_label(backend)
            )

            launch_mpv(
                selected_stream,
                subtitles,
            )

            return True

        print(
            "\n[-] This HLS source "
            "is not playable."
        )

    print(
        "\n[-] Provider "
        f"'{provider}' has no "
        "playable HLS sources."
    )

    return False



# PLAY ONE EPISODE


def play_episode(
    anilist_id,
    episode_number,
    preferred_quality=None,
):
    provider_streams = get_stream_data(
        anilist_id,
        episode_number,
    )

    if not provider_streams:
        print(
            "[-] Could not resolve "
            "any provider streams."
        )
        return False

    for stream_info in provider_streams:
        streams = sort_streams_by_quality(
            stream_info["streams"],
            preferred_quality,
        )

        stream_info = dict(
            stream_info
        )

        stream_info["streams"] = streams

        if play_in_mpv(
            stream_info
        ):
            return True

        print(
            "\n[*] Provider failed. "
            "Trying the next provider..."
        )

    return False



# DOWNLOAD ONE EPISODE


def download_episode(
    anime_title,
    anilist_id,
    episode_number,
    output_dir,
    preferred_quality=None,
):
    print(
        "\n"
        + "#" * 70
    )

    print(
        f"# DOWNLOADING EPISODE "
        f"{episode_number}"
    )

    print(
        "#" * 70
    )

    provider_streams = get_stream_data(
        anilist_id,
        episode_number,
    )

    if not provider_streams:
        print(
            f"[-] No provider data for "
            f"episode {episode_number}."
        )
        return False

    for stream_info in provider_streams:
        provider = stream_info[
            "provider"
        ]

        backend = stream_info[
            "backend"
        ]

        streams = sort_streams_by_quality(
            stream_info["streams"],
            preferred_quality,
        )

        tested_urls = set()

        for stream in streams:
            url = stream.get("url")

            if not url:
                continue

            if url in tested_urls:
                continue

            tested_urls.add(url)

            if not validate_hls_stream(
                stream
            ):
                continue

            print(
                "\n[+] Download source validated!"
            )

            
            # IMPORTANT:
            # Carry provider/backend metadata into
            # the individual stream dictionary.
            

            selected_stream = dict(
                stream
            )

            selected_stream["provider"] = (
                provider
            )

            selected_stream["backend_name"] = (
                backend_label(backend)
            )

            selected_stream["backend"] = (
                backend
            )

            subtitles = stream_info.get(
                "subtitles",
                [],
            )

            subtitle_paths = []

            referer = stream.get(
                "referer"
            )

            headers = build_headers(
                referer
            )

            if headers and subtitles:
                print(
                    f"\n[+] Downloading "
                    f"{len(subtitles)} "
                    f"subtitle track(s)..."
                )

                temp_subtitle_dir = (
                    output_dir
                    / ".subtitle_temp"
                )

                temp_subtitle_dir.mkdir(
                    parents=True,
                    exist_ok=True,
                )

                for (
                    subtitle_index,
                    subtitle,
                ) in enumerate(
                    subtitles,
                    start=1,
                ):
                    subtitle_path = (
                        download_subtitle(
                            subtitle,
                            temp_subtitle_dir,
                            episode_number,
                            subtitle_index,
                            headers,
                        )
                    )

                    if subtitle_path:
                        subtitle_paths.append(
                            subtitle_path
                        )

            success = download_video(
                selected_stream,
                subtitle_paths,
                anime_title,
                episode_number,
                output_dir,
            )

            if success:
                return True

            print(
                f"\n[*] Download failed "
                f"with provider "
                f"'{provider}'."
            )

            print(
                "[*] Trying the next "
                "available source..."
            )

        print(
            f"\n[*] Provider "
            f"'{provider}' has no usable "
            "download source."
        )

    print(
        f"[-] Could not download "
        f"episode {episode_number}."
    )

    return False



# EPISODE PARSING


def parse_episode_spec(
    episode_spec,
    total_episodes,
):
    """
    Accept:

        1
        1-12
        12-24
    """

    if not episode_spec:
        return None

    episode_spec = episode_spec.strip()

    if episode_spec.isdigit():
        episode = int(
            episode_spec
        )

        if (
            episode < 1
            or episode > total_episodes
        ):
            raise ValueError(
                f"Episode must be between "
                f"1 and {total_episodes}."
            )

        return [episode]

    match = re.fullmatch(
        r"(\d+)\s*-\s*(\d+)",
        episode_spec,
    )

    if not match:
        raise ValueError(
            "Invalid episode format. "
            "Use 1 or 1-12."
        )

    start = int(
        match.group(1)
    )

    end = int(
        match.group(2)
    )

    if start < 1 or end < 1:
        raise ValueError(
            "Episode numbers must be positive."
        )

    if start > end:
        raise ValueError(
            "Episode range must be start-end."
        )

    if end > total_episodes:
        raise ValueError(
            f"Episode range cannot exceed "
            f"{total_episodes}."
        )

    return list(
        range(
            start,
            end + 1,
        )
    )



# ARGUMENTS


def parse_arguments():
    parser = argparse.ArgumentParser(
        description=(
            "Anime streaming and downloading "
            "fallback client using Miruro."
        )
    )

    parser.add_argument(
        "title",
        nargs="+",
        help="Anime title",
    )

    parser.add_argument(
        "-e",
        "--episode",
        help=(
            "Episode number or range "
            "(example: 1 or 1-12)"
        ),
    )

    parser.add_argument(
        "-d",
        "--download",
        action="store_true",
        help="Download instead of playing",
    )

    parser.add_argument(
        "-q",
        "--quality",
        help=(
            "Preferred video quality "
            "(example: 1080p)"
        ),
    )

    parser.add_argument(
        "-o",
        "--output",
        default="~/Videos/Anime",
        help=(
            "Download directory "
            "(default: ~/Videos/Anime)"
        ),
    )

    return parser.parse_args()



# MAIN


def main():
    args = parse_arguments()

    query = " ".join(
        args.title
    )

    print(
        f"[*] Searching AniList for: "
        f"{query}..."
    )

    anime = search_anilist(
        query
    )

    if not anime:
        print(
            "[-] Anime not found."
        )
        sys.exit(1)

    anime_title = (
        anime.get("title", {})
        .get("romaji")
        or anime.get("title", {})
        .get("english")
        or query
    )

    total_episodes = (
        anime.get("episodes")
        or 1
    )

    anilist_id = anime.get(
        "id"
    )

    print(
        f"[+] Found: {anime_title} "
        f"(ID: {anilist_id}, "
        f"Episodes: {total_episodes})"
    )


    # EPISODES

    if args.episode:
        try:
            episodes = parse_episode_spec(
                args.episode,
                total_episodes,
            )

        except ValueError as exc:
            print(
                f"[-] {exc}"
            )
            sys.exit(1)

    else:
        try:
            episode = int(
                input(
                    f"Enter episode number "
                    f"(1-{total_episodes}): "
                ).strip()
            )

            if (
                episode < 1
                or episode > total_episodes
            ):
                raise ValueError

            episodes = [episode]

        except ValueError:
            print(
                "[-] Invalid episode number."
            )
            sys.exit(1)

    output_dir = Path(
        os.path.expanduser(
            args.output
        )
    )

    # --------------------------------------------------------
    # DOWNLOAD MODE
    # --------------------------------------------------------

    if args.download:
        print(
            "\n"
            + "=" * 60
        )

        print(
            "[+] DOWNLOAD MODE"
        )

        print(
            f"[+] Episodes: "
            f"{episodes[0]}-"
            f"{episodes[-1]}"
        )

        print(
            f"[+] Output: "
            f"{output_dir}"
        )

        print(
            "=" * 60
        )

        failed = []

        for episode_number in episodes:
            success = download_episode(
                anime_title,
                anilist_id,
                episode_number,
                output_dir,
                args.quality,
            )

            if not success:
                failed.append(
                    episode_number
                )

        print(
            "\n"
            + "=" * 60
        )

        print(
            "[+] DOWNLOAD RUN FINISHED"
        )

        if failed:
            print(
                f"[-] Failed episodes: "
                f"{', '.join(map(str, failed))}"
            )
        else:
            print(
                "[+] Every requested episode "
                "was downloaded successfully."
            )

        print(
            "=" * 60
        )

        sys.exit(
            1 if failed else 0
        )


    # PLAY MODE
    

    for episode_number in episodes:
        print(
            "\n"
            + "=" * 60
        )

        print(
            f"[+] PLAYING EPISODE "
            f"{episode_number}"
        )

        print(
            "=" * 60
        )

        success = play_episode(
            anilist_id,
            episode_number,
            args.quality,
        )

        if not success:
            print(
                f"[-] Episode "
                f"{episode_number} "
                f"could not be played."
            )

            if len(episodes) > 1:
                print(
                    "[*] Continuing to "
                    "the next episode..."
                )
                continue

            sys.exit(1)


if __name__ == "__main__":
    main()