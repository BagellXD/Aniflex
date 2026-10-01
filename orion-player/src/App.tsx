import { Component, memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent, ReactElement, ReactNode } from "react";
import {
  ArrowLeft,
  Bell,
  Captions,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Film,
  Gauge,
  Info,
  List,
  Maximize2,
  Minimize2,
  Pause,
  Play,
  Plus,
  RotateCcw,
  RotateCw,
  Search,
  SkipForward,
  Sparkles,
  ThumbsDown,
  ThumbsUp,
  Volume2,
  VolumeX,
  X,
} from "lucide-react";
import catalogData from "../data/catalog.json";
import AnimeRequest from "./components/AnimeRequest";
import "./style.css";

/* ================================================================== */
/*  Types                                                             */
/* ================================================================== */

type Episode = { number: number; season?: number; video: string; subtitle?: string };
type Rating = "none" | "liked" | "disliked";

type CatalogEntry = {
  id: string;
  title: string;
  year?: number;
  format?: "TV" | "Movie" | "OVA" | "ONA";
  genres?: string[];
  synopsis?: string;
  poster?: string;
  video?: string;
  subtitle?: string;
  historyId?: string;
  rating?: Rating;
  episodes?: Episode[];
};

type FormatFilter = "all" | "TV" | "Movie" | "OVA" | "ONA";
type SortMode = "title" | "year" | "episodes";
type View = "home" | "browse" | "list" | "add" | "manga";
type Sprite = { url: string; interval: number; width: number; height: number; columns: number };
type LocalProgress = { episode: number; position: number; duration: number; at: number };

/* ================================================================== */
/*  Constants & helpers                                               */
/* ================================================================== */

const BRAND = "ANIFLEX";
const LIBRARY_LIMIT = 40;
const BOOKMARK_KEY = "orion-bookmarks";
const CONTINUE_KEY = "aniflix-continue";
const SPEEDS = [0.75, 1, 1.25, 1.5, 2];
const INTRO_MS = 4600;

/*
 * netflix.mp3 lives at orion-player/src/netflix.mp3. Vite bundles it, so any
 * server that serves the built app will serve the sound too. The glob checks
 * a few relative spots so it works wherever this file sits under src/.
 * Falls back to /netflix.mp3 (public folder) if none match.
 */
const soundModules = import.meta.glob<{ default: string }>(
  ["/src/netflix.mp3", "../netflix.mp3", "./netflix.mp3"],
  { eager: true }
);
const INTRO_SOUND = Object.values(soundModules)[0]?.default ?? "/netflix.mp3";

function getEpisodes(item: CatalogEntry): Episode[] {
  if (item.episodes && item.episodes.length > 0) return item.episodes;
  if (item.video) return [{ number: 1, video: item.video, subtitle: item.subtitle }];
  return [];
}

const seasonOf = (ep: Episode) => ep.season ?? 1;
const seasonsOf = (item: CatalogEntry) =>
  [...new Set(getEpisodes(item).map(seasonOf))].sort((a, b) => a - b);
const epLabel = (ep: Episode) => `S${seasonOf(ep)}:E${ep.number}`;

function getPosterUrl(item: CatalogEntry): string {
  if (item.poster && /^(https?:)?\//.test(item.poster)) return item.poster;
  return `/media/${encodeURIComponent(item.id)}/poster.jpg`;
}

const getVideoUrl = (item: CatalogEntry, ep: Episode) =>
  `/video/${encodeURIComponent(item.id)}/${encodeURIComponent(ep.video)}`;

const getSubtitleUrl = (item: CatalogEntry, ep: Episode) =>
  `/subtitles/${encodeURIComponent(item.id)}/${encodeURIComponent(
    ep.subtitle ?? (ep.video ?? "").replace(/\.[^.]+$/, ".vtt")
  )}`;

function readJSON<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

function writeJSON(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* private mode / quota */
  }
}

const loadBookmarks = (): string[] => {
  const value = readJSON<unknown>(BOOKMARK_KEY, []);
  return Array.isArray(value) ? value.filter((x): x is string => typeof x === "string") : [];
};

const loadContinue = (): Record<string, LocalProgress> => {
  const raw = readJSON<unknown>(CONTINUE_KEY, {});
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};

  const out: Record<string, LocalProgress> = {};
  for (const [id, value] of Object.entries(raw as Record<string, unknown>)) {
    if (!value || typeof value !== "object") continue;
    const v = value as Record<string, unknown>;
    const episode = Number(v.episode);
    const position = Number(v.position);
    const duration = Number(v.duration);
    const at = Number(v.at);
    if (!Number.isFinite(episode) || !Number.isFinite(position)) continue;
    out[id] = { episode, position, duration: Number.isFinite(duration) ? duration : 0, at: Number.isFinite(at) ? at : 0 };
  }
  return out;
};

/*
 * The catalog can come from data/catalog.json (written by the Rust generator:
 * episodes look like { "episode": 3 }) or from /api/catalog (episodes look like
 * { "number": 3, "video": "..." }). Normalizing both into one safe shape means
 * a single odd entry can never crash the whole UI.
 */
const FORMATS = ["TV", "Movie", "OVA", "ONA"] as const;

function normalizeEpisodes(raw: unknown): Episode[] {
  if (!Array.isArray(raw)) return [];

  const seen = new Set<string>();
  const out: Episode[] = [];

  raw.forEach((value, index) => {
    if (!value || typeof value !== "object") return;
    const e = value as Record<string, unknown>;

    const number = Number(e.number ?? e.episode ?? index + 1);
    if (!Number.isFinite(number)) return;

    const season = Number(e.season);
    const validSeason = Number.isFinite(season) && season > 0;
    const key = `${validSeason ? season : 1}:${number}`;
    if (seen.has(key)) return;
    seen.add(key);

    out.push({
      number,
      season: validSeason ? season : undefined,
      video: typeof e.video === "string" ? e.video : typeof e.file === "string" ? e.file : "",
      subtitle: typeof e.subtitle === "string" ? e.subtitle : undefined,
    });
  });

  return out.sort((a, b) => seasonOf(a) - seasonOf(b) || a.number - b.number);
}

function normalizeCatalog(raw: unknown): CatalogEntry[] {
  if (!Array.isArray(raw)) return [];

  const seen = new Set<string>();
  const out: CatalogEntry[] = [];

  for (const value of raw) {
    if (!value || typeof value !== "object") continue;
    const e = value as Record<string, unknown>;

    const title = typeof e.title === "string" ? e.title.trim() : "";
    const id =
      typeof e.id === "string" && e.id
        ? e.id
        : title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
    if (!title || !id || seen.has(id)) continue;
    seen.add(id);

    const year = Number(e.year);

    out.push({
      id,
      title,
      year: Number.isFinite(year) && year > 0 ? year : undefined,
      format: FORMATS.find((f) => f === e.format),
      genres: Array.isArray(e.genres) ? e.genres.filter((g): g is string => typeof g === "string") : [],
      synopsis: typeof e.synopsis === "string" ? e.synopsis : undefined,
      poster: typeof e.poster === "string" ? e.poster : undefined,
      video: typeof e.video === "string" ? e.video : undefined,
      subtitle: typeof e.subtitle === "string" ? e.subtitle : undefined,
      historyId: typeof e.historyId === "string" ? e.historyId : undefined,
      rating: e.rating === "liked" || e.rating === "disliked" ? (e.rating as Rating) : undefined,
      episodes: normalizeEpisodes(e.episodes),
    });
  }

  return out;
}

function setLocalProgress(id: string, episode: number, position: number, duration: number) {
  const all = loadContinue();
  all[id] = { episode, position, duration: Number.isFinite(duration) ? duration : 0, at: Date.now() };
  writeJSON(CONTINUE_KEY, all);
}

function clearLocalProgress(id: string) {
  const all = loadContinue();
  delete all[id];
  writeJSON(CONTINUE_KEY, all);
}

function formatTime(s: number): string {
  if (!Number.isFinite(s)) return "0:00";
  const t = Math.max(0, Math.floor(s));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const r = t % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}`
    : `${m}:${String(r).padStart(2, "0")}`;
}

function shuffle<T>(arr: T[]): T[] {
  const a = [...arr];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

function matchScore(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) % 997;
  return 90 + (h % 10);
}

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

type Note = { id: number; text: string; at: number; unread: boolean };

function timeAgo(at: number): string {
  const s = Math.max(0, Math.floor((Date.now() - at) / 1000));
  if (s < 45) return "Just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${Math.max(1, m)}m ago`;
  const h = Math.floor(m / 60);
  return h < 24 ? `${h}h ago` : `${Math.floor(h / 24)}d ago`;
}

/* ================================================================== */
/*  Playback API                                                      */
/* ================================================================== */

function postProgress(id: string, episode: number, position: number) {
  return fetch("/api/progress", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ id, episode, position }),
    keepalive: true,
  }).catch((e) => console.error("Failed to save playback position:", e));
}

async function fetchSavedPosition(id: string, episode: number): Promise<number | null> {
  try {
    const response = await fetch(`/api/progress/${encodeURIComponent(id)}/${episode}`);
    if (response.ok) {
      const data = (await response.json()) as { position?: number };
      const position = Number(data?.position);
      return Number.isFinite(position) && position > 0 ? position : null;
    }
  } catch {
    /* fall through to local copy */
  }
  const local = loadContinue()[id];
  return local && local.episode === episode && local.position > 0 ? local.position : null;
}

/* ================================================================== */
/*  Intro                                                             */
/* ================================================================== */

/* Built-in "tu-dum" used only if netflix.mp3 can't be loaded or played. */
function playTudum() {
  try {
    const Ctx = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctx) return;
    const ctx = new Ctx();
    const t0 = ctx.currentTime + 0.05;

    const hit = (at: number, freq: number, peak: number, len: number) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.type = "sine";
      osc.frequency.setValueAtTime(freq * 1.6, at);
      osc.frequency.exponentialRampToValueAtTime(freq, at + 0.12);
      gain.gain.setValueAtTime(0.0001, at);
      gain.gain.exponentialRampToValueAtTime(peak, at + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, at + len);
      osc.connect(gain).connect(ctx.destination);
      osc.start(at);
      osc.stop(at + len + 0.05);
    };

    hit(t0, 62, 0.8, 0.9);
    hit(t0 + 0.42, 55, 1, 2.6);
    hit(t0 + 0.42, 110, 0.35, 2.2);
    window.setTimeout(() => void ctx.close().catch(() => {}), 4500);
  } catch {
    /* no audio available */
  }
}

const STREAK_HUES = [355, 0, 215, 265, 190, 340, 235];

function Intro({ onDone }: { onDone: () => void }) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const startedRef = useRef(false);
  const leavingRef = useRef(false);
  const timers = useRef<number[]>([]);
  const [started, setStarted] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const [soundMissing, setSoundMissing] = useState(false);

  const finish = useCallback(() => {
    if (leavingRef.current) return;
    leavingRef.current = true;
    setLeaving(true);

    const audio = audioRef.current;
    if (audio && !audio.paused) {
      const id = window.setInterval(() => {
        if (audio.volume > 0.08) {
          audio.volume = Math.max(0, audio.volume - 0.08);
        } else {
          window.clearInterval(id);
          audio.pause();
        }
      }, 40);
      timers.current.push(id);
    }
    timers.current.push(window.setTimeout(onDone, 600));
  }, [onDone]);

  const begin = useCallback(
    (audioAlreadyPlaying: boolean) => {
      if (startedRef.current || leavingRef.current) return;
      startedRef.current = true;
      setStarted(true);

      const audio = audioRef.current;
      if (!audioAlreadyPlaying) {
        const broken = !audio || audio.error !== null || audio.networkState === 3;
        if (broken) {
          setSoundMissing(true);
          playTudum();
        } else {
          audio.currentTime = 0;
          audio.volume = 0.9;
          audio.play().catch((error) => {
            console.error("netflix.mp3 could not play:", error);
            setSoundMissing(true);
            playTudum();
          });
        }
      }
      timers.current.push(window.setTimeout(finish, INTRO_MS));
    },
    [finish]
  );

  useEffect(() => {
    const audio = audioRef.current;
    let cancelled = false;

    if (audio) {
      audio.volume = 0.9;
      /* Works when the browser allows autoplay; otherwise the user clicks to start. */
      audio
        .play()
        .then(() => {
          if (!cancelled) begin(true);
        })
        .catch(() => {});
    }

    return () => {
      cancelled = true;
      timers.current.forEach((id) => {
        window.clearTimeout(id);
        window.clearInterval(id);
      });
      timers.current = [];
      startedRef.current = false;
      leavingRef.current = false;
      audio?.pause();
    };
  }, [begin]);

  return (
    <div className={`nf-intro${started ? " started" : ""}${leaving ? " leaving" : ""}`}>
      <audio ref={audioRef} src={INTRO_SOUND} preload="auto" onError={() => setSoundMissing(true)} />

      {!started && (
        <div className="nf-intro-start">
          <span className="nf-intro-start-logo">{BRAND}</span>
          {soundMissing && (
            <span style={{ color: "#ff8a8a", fontSize: 12, letterSpacing: ".04em", textTransform: "none" }}>
              Couldn’t load {INTRO_SOUND} — check that it is at src/netflix.mp3. A built-in sound will play instead.
            </span>
          )}
          <button className="nf-intro-force" onClick={() => begin(false)} type="button">
            Force
          </button>
        </div>
      )}

      <div className="nf-intro-stage" aria-hidden>
        <div className="nf-intro-glow" />

        <div className="nf-logo-wrap">
          <svg className="nf-mono" viewBox="0 0 100 120">
            <defs>
              <linearGradient id="nfLeg" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stopColor="#ff3b46" />
                <stop offset="1" stopColor="#a10610" />
              </linearGradient>
              <linearGradient id="nfBar" x1="0" y1="0" x2="1" y2="0">
                <stop offset="0" stopColor="#7d040b" />
                <stop offset="0.5" stopColor="#e50914" />
                <stop offset="1" stopColor="#7d040b" />
              </linearGradient>
            </defs>
            <polygon className="leg l" points="30,0 56,0 26,120 0,120" fill="url(#nfLeg)" />
            <polygon className="leg r" points="44,0 70,0 100,120 74,120" fill="url(#nfLeg)" />
            <polygon className="bar" points="31,78 69,78 71,96 29,96" fill="url(#nfBar)" />
          </svg>

          <div className="nf-word">
            {BRAND.split("").map((c, i) => (
              <span key={i} style={{ ["--i" as string]: i } as CSSProperties}>
                {c}
              </span>
            ))}
          </div>
        </div>

        <div className="nf-streaks">
          {Array.from({ length: 30 }, (_, i) => (
            <span
              key={i}
              style={
                { ["--i" as string]: i, ["--h" as string]: STREAK_HUES[i % STREAK_HUES.length] } as CSSProperties
              }
            />
          ))}
        </div>
      </div>

      <button className="nf-intro-skip" onClick={finish} type="button">
        Skip intro
      </button>
    </div>
  );
}

/* ================================================================== */
/*  Seek preview (sprite sheet)                                       */
/* ================================================================== */

const PREVIEW_W = 176;

function SeekPreview({ sprite, time }: { sprite: Sprite; time: number }) {
  const scale = PREVIEW_W / sprite.width;
  const index = Math.max(0, Math.floor(time / Math.max(0.1, sprite.interval)));
  const column = index % sprite.columns;
  const row = Math.floor(index / sprite.columns);

  return (
    <div className="nf-preview-frame" style={{ width: PREVIEW_W, height: sprite.height * scale }}>
      <div
        style={{
          width: sprite.width,
          height: sprite.height,
          backgroundImage: `url("${sprite.url}")`,
          backgroundPosition: `-${column * sprite.width}px -${row * sprite.height}px`,
          transform: `scale(${scale})`,
          transformOrigin: "top left",
        }}
      />
    </div>
  );
}

/* ================================================================== */
/*  Player — Netflix replica                                          */
/* ================================================================== */

type PlayerProps = {
  playing: CatalogEntry;
  selectedEpisode: Episode;
  episodes: Episode[];
  historyBusy: boolean;
  historyError: string;
  onSelectEpisode: (episode: Episode) => void;
  onHistory: (liked: boolean) => void;
  onClose: () => void;
};

function Player({
  playing,
  selectedEpisode,
  episodes,
  historyBusy,
  historyError,
  onSelectEpisode,
  onHistory,
  onClose,
}: PlayerProps) {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const playerRef = useRef<HTMLDivElement | null>(null);
  const seekRef = useRef<HTMLDivElement | null>(null);
  const controlsTimer = useRef<number | null>(null);
  const flashTimer = useRef<number | null>(null);
  const restoredRef = useRef(false);
  const interactedRef = useRef(false);
  const endedRef = useRef(false);
  const lastSavedAt = useRef(0);
  const lastSavedPos = useRef(-1);
  const scrubbingRef = useRef(false);
  const uiHiddenRef = useRef(false);
  const nextRef = useRef<() => void>(() => {});

  const [isPlaying, setIsPlaying] = useState(false);
  const [videoError, setVideoError] = useState(false);
  const [retryKey, setRetryKey] = useState(0);
  const [buffering, setBuffering] = useState(true);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [rate, setRate] = useState(1);
  const [captions, setCaptions] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [showControls, setShowControls] = useState(true);
  /* true = the person tapped the video to hide the whole UI (stays hidden until the next tap) */
  const [uiHidden, setUiHiddenState] = useState(false);
  const [showEpisodes, setShowEpisodes] = useState(false);
  const [nextCancelled, setNextCancelled] = useState(false);
  const [resumedAt, setResumedAt] = useState<number | null>(null);
  const [flash, setFlash] = useState<"play" | "pause" | null>(null);
  const [showBigTitle, setShowBigTitle] = useState(true);
  const [sprite, setSprite] = useState<Sprite | null>(null);
  const [hover, setHover] = useState<{ x: number; t: number; w: number } | null>(null);
  const [scrubbing, setScrubbing] = useState(false);
  const [scrubTime, setScrubTime] = useState(0);

  const setUiHidden = (value: boolean) => {
    uiHiddenRef.current = value;
    setUiHiddenState(value);
  };

  const currentIndex = episodes.findIndex((e) => e.number === selectedEpisode.number);
  const canGoNext = currentIndex >= 0 && currentIndex < episodes.length - 1;
  const nextEpisode = canGoNext ? episodes[currentIndex + 1] : null;
  const episodeKey = `${playing.id}-${selectedEpisode.number}-${retryKey}`;
  const videoSrc = selectedEpisode.video ? getVideoUrl(playing, selectedEpisode) : "";
  const missingVideo = videoSrc === "";

  const showUi = !uiHidden && (showControls || !isPlaying || showEpisodes);

  const persist = (video: HTMLVideoElement, id: string, episode: number, force: boolean) => {
    if (!restoredRef.current || endedRef.current) return;
    const position = video.currentTime;
    if (!Number.isFinite(position) || position < 0) return;

    const now = Date.now();
    if (!force && (now - lastSavedAt.current < 1500 || Math.abs(position - lastSavedPos.current) < 0.5)) return;

    lastSavedAt.current = now;
    lastSavedPos.current = position;
    void postProgress(id, episode, position);
    setLocalProgress(id, episode, position, video.duration);
  };

  /* show the UI (also un-hides it) and start the auto-hide countdown */
  const revealControls = () => {
    if (uiHiddenRef.current) setUiHidden(false);
    setShowControls(true);
    if (controlsTimer.current) window.clearTimeout(controlsTimer.current);
    controlsTimer.current = window.setTimeout(() => setShowControls(false), 3000);
  };

  /* hide everything until the next tap on the video */
  const hideUi = () => {
    if (controlsTimer.current) window.clearTimeout(controlsTimer.current);
    setShowControls(false);
    setUiHidden(true);
  };

  const saveNow = () => {
    const video = videoRef.current;
    if (video) persist(video, playing.id, selectedEpisode.number, true);
  };

  const requestClose = () => {
    saveNow();
    onClose();
  };

  const switchEpisode = (episode: Episode) => {
    saveNow();
    onSelectEpisode(episode);
  };

  const goNext = () => {
    if (canGoNext) switchEpisode(episodes[currentIndex + 1]);
  };

  nextRef.current = () => {
    if (canGoNext && !nextCancelled) onSelectEpisode(episodes[currentIndex + 1]);
  };

  const togglePlay = () => {
    const video = videoRef.current;
    if (!video) return;
    interactedRef.current = true;

    if (video.paused) {
      if (video.readyState === HTMLMediaElement.HAVE_NOTHING) video.load();

      void video.play().then(
        () => {
          setVideoError(false);
          setFlash("play");
        },
        (error) => {
          console.error("ANIFLEX video play failed:", {
            error,
            src: video.currentSrc || video.src,
            mediaError: video.error,
            readyState: video.readyState,
            networkState: video.networkState,
          });
          setBuffering(false);
          setVideoError(true);
        }
      );
    } else {
      video.pause();
      setFlash("pause");
    }

    revealControls();
    if (flashTimer.current) window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setFlash(null), 500);
  };

  const skip = (delta: number) => {
    const video = videoRef.current;
    if (!video) return;
    interactedRef.current = true;
    video.currentTime = clamp(video.currentTime + delta, 0, video.duration || Infinity);
    revealControls();
  };

  const toggleFullscreen = async () => {
    const element = playerRef.current;
    if (!element) return;
    try {
      if (typeof element.requestFullscreen !== "function") {
        /* iPhone Safari can't fullscreen a div — use the native video fullscreen instead */
        const video = videoRef.current as (HTMLVideoElement & { webkitEnterFullscreen?: () => void }) | null;
        video?.webkitEnterFullscreen?.();
        return;
      }
      if (!document.fullscreenElement) await element.requestFullscreen();
      else await document.exitFullscreen();
    } catch (error) {
      console.error("Fullscreen failed:", error);
    }
  };

  /*
   * Tap / click on the video (not on a button): toggle the whole UI.
   * It never pauses or plays. If the episode panel is open, the first tap just closes it.
   */
  const onVideoClick = () => {
    if (showEpisodes) {
      setShowEpisodes(false);
      return;
    }
    if (showUi) hideUi();
    else revealControls();
  };
  const onVideoDoubleClick = () => void toggleFullscreen();

  /* seek bar */
  const seekPoint = (clientX: number) => {
    const element = seekRef.current;
    if (!element || duration <= 0) return null;
    const rect = element.getBoundingClientRect();
    if (rect.width <= 0) return null;
    const pct = clamp((clientX - rect.left) / rect.width, 0, 1);
    return { x: pct * rect.width, t: pct * duration, w: rect.width };
  };

  const onSeekDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    const point = seekPoint(event.clientX);
    if (!point) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    interactedRef.current = true;
    scrubbingRef.current = true;
    setScrubbing(true);
    setScrubTime(point.t);
    setHover(point);
  };

  const onSeekMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const point = seekPoint(event.clientX);
    if (!point) return;
    setHover(point);
    if (scrubbingRef.current) setScrubTime(point.t);
  };

  const commitSeek = (event: ReactPointerEvent<HTMLDivElement>, cancel: boolean) => {
    if (!scrubbingRef.current) return;
    scrubbingRef.current = false;
    setScrubbing(false);

    const point = seekPoint(event.clientX);
    const video = videoRef.current;
    if (!cancel && point && video) {
      video.currentTime = point.t;
      setCurrentTime(point.t);
    }
    /* touch has no hover, so drop the preview once the finger lifts */
    if (event.pointerType !== "mouse") setHover(null);
    revealControls();
  };

  /* main media lifecycle: metadata, restore, persistence */
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const animeId = playing.id;
    const episodeNumber = selectedEpisode.number;
    let cancelled = false;
    let saved: number | null = null;
    let savedLoaded = false;

    restoredRef.current = false;
    interactedRef.current = false;
    endedRef.current = false;
    lastSavedAt.current = 0;
    lastSavedPos.current = -1;

    setVideoError(false);
    setIsPlaying(false);
    setCurrentTime(0);
    setDuration(0);
    setBuffering(true);
    setShowControls(true);
    setUiHidden(false);
    setShowEpisodes(false);
    setNextCancelled(false);
    setResumedAt(null);
    setHover(null);
    setScrubbing(false);
    scrubbingRef.current = false;

    const tryRestore = () => {
      if (cancelled || restoredRef.current || !savedLoaded || video.readyState < 1) return;

      const total = video.duration;
      const usable =
        saved !== null && !interactedRef.current && (!Number.isFinite(total) || total <= 0 || saved < total - 5);

      if (usable && saved !== null) {
        video.currentTime = saved;
        setCurrentTime(saved);
        setResumedAt(saved);
        window.setTimeout(() => {
          if (!cancelled) setResumedAt(null);
        }, 4000);
      }
      restoredRef.current = true;
    };

    void fetchSavedPosition(animeId, episodeNumber).then((position) => {
      saved = position;
      savedLoaded = true;
      tryRestore();
    });

    const onMetadata = () => {
      setDuration(Number.isFinite(video.duration) ? video.duration : 0);
      setVideoError(false);
      tryRestore();
    };
    const onTimeUpdate = () => {
      if (!scrubbingRef.current) setCurrentTime(video.currentTime);
    };
    const onPlay = () => {
      setIsPlaying(true);
      setVideoError(false);
      setBuffering(false);
    };
    const onPause = () => {
      setIsPlaying(false);
      setShowControls(true);
      if (!video.ended) persist(video, animeId, episodeNumber, true);
    };
    const onWaiting = () => setBuffering(true);
    const onReady = () => {
      setVideoError(false);
      setBuffering(false);
    };
    const onMediaError = () => {
      const mediaError = video.error;
      console.error("ANIFLEX video media error:", {
        src: video.currentSrc || video.src,
        code: mediaError?.code,
        message: mediaError?.message,
        readyState: video.readyState,
        networkState: video.networkState,
      });
      setBuffering(false);
      setVideoError(true);
    };
    const onSeeked = () => {
      setCurrentTime(video.currentTime);
      persist(video, animeId, episodeNumber, true);
    };
    const onEnded = () => {
      endedRef.current = true;
      void postProgress(animeId, episodeNumber, 0);
      clearLocalProgress(animeId);
      setIsPlaying(false);
      setShowControls(true);
      setUiHidden(false);
      nextRef.current();
    };
    const onHide = () => {
      if (document.visibilityState === "hidden") persist(video, animeId, episodeNumber, true);
    };
    const onPageHide = () => persist(video, animeId, episodeNumber, true);

    video.addEventListener("loadedmetadata", onMetadata);
    video.addEventListener("durationchange", onMetadata);
    video.addEventListener("timeupdate", onTimeUpdate);
    video.addEventListener("play", onPlay);
    video.addEventListener("pause", onPause);
    video.addEventListener("waiting", onWaiting);
    video.addEventListener("playing", onReady);
    video.addEventListener("canplay", onReady);
    video.addEventListener("loadeddata", onReady);
    video.addEventListener("seeked", onSeeked);
    video.addEventListener("error", onMediaError);
    video.addEventListener("ended", onEnded);
    document.addEventListener("visibilitychange", onHide);
    window.addEventListener("pagehide", onPageHide);

    if (video.readyState >= 1) onMetadata();

    const timer = window.setInterval(() => {
      if (!video.paused && !video.ended) persist(video, animeId, episodeNumber, false);
    }, 2000);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
      video.removeEventListener("loadedmetadata", onMetadata);
      video.removeEventListener("durationchange", onMetadata);
      video.removeEventListener("timeupdate", onTimeUpdate);
      video.removeEventListener("play", onPlay);
      video.removeEventListener("pause", onPause);
      video.removeEventListener("waiting", onWaiting);
      video.removeEventListener("playing", onReady);
      video.removeEventListener("canplay", onReady);
      video.removeEventListener("loadeddata", onReady);
      video.removeEventListener("seeked", onSeeked);
      video.removeEventListener("error", onMediaError);
      video.removeEventListener("ended", onEnded);
      document.removeEventListener("visibilitychange", onHide);
      window.removeEventListener("pagehide", onPageHide);
      persist(video, animeId, episodeNumber, true);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [episodeKey]);

  useEffect(() => {
    setShowBigTitle(true);
    const timer = window.setTimeout(() => setShowBigTitle(false), 3000);
    return () => window.clearTimeout(timer);
  }, [episodeKey]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    video.volume = volume;
    video.muted = muted;
    video.playbackRate = rate;
  }, [volume, muted, rate, episodeKey]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const apply = () => {
      for (let i = 0; i < video.textTracks.length; i++) {
        video.textTracks[i].mode = captions ? "showing" : "hidden";
      }
    };
    apply();
    video.textTracks.addEventListener("addtrack", apply);
    return () => video.textTracks.removeEventListener("addtrack", apply);
  }, [captions, episodeKey]);

  useEffect(() => {
    let cancelled = false;
    setSprite(null);

    fetch(`/api/thumbnails/${encodeURIComponent(playing.id)}/${selectedEpisode.number}`)
      .then((response) => (response.ok ? response.json() : null))
      .then((data: Sprite | null) => {
        if (!cancelled && data && data.url && data.width > 0 && data.height > 0 && data.columns > 0 && data.interval > 0) {
          setSprite(data);
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [playing.id, selectedEpisode.number]);

  useEffect(() => {
    const onFullscreenChange = () => setFullscreen(Boolean(document.fullscreenElement));
    document.addEventListener("fullscreenchange", onFullscreenChange);

    return () => {
      document.removeEventListener("fullscreenchange", onFullscreenChange);
      if (controlsTimer.current) window.clearTimeout(controlsTimer.current);
      if (flashTimer.current) window.clearTimeout(flashTimer.current);
    };
  }, []);

  /* keyboard */
  const keyHandler = useRef<(event: KeyboardEvent) => void>(() => {});
  keyHandler.current = (event) => {
    const target = event.target;
    if (target instanceof HTMLInputElement || target instanceof HTMLSelectElement || target instanceof HTMLTextAreaElement) return;
    if (target instanceof HTMLButtonElement && (event.key === " " || event.key === "Enter")) return;
    if (event.metaKey || event.ctrlKey || event.altKey) return;

    switch (event.key.toLowerCase()) {
      case " ":
      case "k":
        event.preventDefault();
        togglePlay();
        break;
      case "m":
        setMuted((v) => !v);
        break;
      case "c":
        setCaptions((v) => !v);
        break;
      case "f":
        void toggleFullscreen();
        break;
      case "n":
        goNext();
        break;
      case "arrowleft":
      case "j":
        event.preventDefault();
        skip(-10);
        break;
      case "arrowright":
      case "l":
        event.preventDefault();
        skip(10);
        break;
      case "arrowup":
        event.preventDefault();
        setMuted(false);
        setVolume((v) => clamp(v + 0.1, 0, 1));
        break;
      case "arrowdown":
        event.preventDefault();
        setVolume((v) => clamp(v - 0.1, 0, 1));
        break;
      case "escape":
        if (showEpisodes) setShowEpisodes(false);
        else requestClose();
        break;
    }
  };

  useEffect(() => {
    const handler = (event: KeyboardEvent) => keyHandler.current(event);
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const shownTime = scrubbing ? scrubTime : currentTime;
  const remaining = Math.max(0, duration - shownTime);
  const nextCountdown =
    isPlaying && canGoNext && !nextCancelled && duration > 0 && remaining > 0 && remaining <= 10
      ? Math.ceil(remaining)
      : null;
  const progressPct = duration > 0 ? clamp((shownTime / duration) * 100, 0, 100) : 0;
  const paused = !isPlaying && !videoError && !missingVideo && !buffering;
  const previewLeft = hover
    ? clamp(hover.x, PREVIEW_W / 2 + 8, Math.max(PREVIEW_W / 2 + 8, hover.w - PREVIEW_W / 2 - 8))
    : 0;

  const upcoming = useMemo(
    () => (currentIndex >= 0 ? episodes.slice(currentIndex + 1, currentIndex + 5) : []),
    [episodes, currentIndex]
  );

  const seasons = useMemo(() => [...new Set(episodes.map(seasonOf))].sort((a, b) => a - b), [episodes]);

  return (
    <div className="nf-player-wrap">
      <div
        ref={playerRef}
        className={`nf-player${showUi ? " ui" : ""}`}
        onPointerMove={(event) => {
          /* only a real mouse wakes the UI; a tap on touch is handled by the video click */
          if (event.pointerType === "mouse" && !uiHiddenRef.current) revealControls();
        }}
        onMouseLeave={() => isPlaying && setShowControls(false)}
      >
        {videoError || missingVideo ? (
          <div className="nf-video-error">
            <Film size={44} />
            <h3>Video unavailable</h3>
            <p>
              {missingVideo
                ? "The catalog doesn’t list a video file for this episode."
                : "The media server could not serve this episode."}
            </p>
            <button
              className="nf-btn white"
              onClick={() => {
                setVideoError(false);
                setRetryKey((v) => v + 1);
              }}
              type="button"
            >
              <RotateCcw size={15} /> Try again
            </button>
            <button className="nf-btn grey" onClick={requestClose} type="button">
              Back to browsing
            </button>
          </div>
        ) : (
          <video
            ref={videoRef}
            key={episodeKey}
            className="nf-video"
            src={videoSrc}
            autoPlay
            playsInline
            preload="auto"
            disablePictureInPicture
            controlsList="nodownload"
            onClick={onVideoClick}
            onDoubleClick={onVideoDoubleClick}
            onError={() => {
              setBuffering(false);
              setVideoError(true);
            }}
          >
            <track kind="subtitles" src={getSubtitleUrl(playing, selectedEpisode)} srcLang="en" label="English" />
          </video>
        )}

        {buffering && !videoError && !missingVideo && <div className="nf-spinner" aria-label="Loading" />}

        {flash && !videoError && (
          <div className="nf-flash">
            {flash === "play" ? <Play size={40} fill="currentColor" /> : <Pause size={40} fill="currentColor" />}
          </div>
        )}

        {showBigTitle && !videoError && !paused && (
          <div className="nf-bigtitle">
            <strong>{playing.title}</strong>
            <span>{epLabel(selectedEpisode)}: Episode {selectedEpisode.number}</span>
          </div>
        )}

        {resumedAt !== null && (
          <div className="nf-resumed">
            <RotateCcw size={14} /> Resumed from {formatTime(resumedAt)}
          </div>
        )}

        <div className="nf-top">
          <button className="nf-icon" onClick={requestClose} title="Back (Esc)" type="button" aria-label="Back">
            <ArrowLeft size={32} />
          </button>

          <div className="nf-top-title">
            <strong>{playing.title}</strong>
            <span>
              {epLabel(selectedEpisode)}: Episode {selectedEpisode.number}
            </span>
          </div>

          <div className="nf-top-actions">
            <button
              className={`nf-circle${playing.rating === "liked" ? " on" : ""}`}
              disabled={historyBusy}
              onClick={() => onHistory(true)}
              title="I like this"
              type="button"
              aria-label="I like this"
            >
              <ThumbsUp size={18} />
            </button>
            <button
              className={`nf-circle${playing.rating === "disliked" ? " on" : ""}`}
              disabled={historyBusy}
              onClick={() => onHistory(false)}
              title="Not for me"
              type="button"
              aria-label="Not for me"
            >
              <ThumbsDown size={18} />
            </button>
          </div>
        </div>

        {historyError && <div className="nf-history-error">{historyError}</div>}

        {/* Big centre play button (touch devices only, via CSS) */}
        {paused && showUi && !showEpisodes && (
          <button className="nf-center-play" onClick={togglePlay} type="button" aria-label="Play">
            <Play size={34} fill="currentColor" />
          </button>
        )}

        {/* Netflix pause screen: left info panel, right up-next card */}
        {paused && showUi && !showEpisodes && (
          <div className="pv-pause-layer">
            <div className="pv-pause-info">
              <span className="pv-series">{playing.title}</span>
              <h2>{epLabel(selectedEpisode)}: Episode {selectedEpisode.number}</h2>
              <p>{playing.synopsis ?? ""}</p>
            </div>

            {nextEpisode && (
              <div className="pv-upnext-card">
                <span className="pv-upnext-label">Up Next</span>
                <button className="pv-upnext-thumb" onClick={goNext} type="button">
                  <img src={getPosterUrl(playing)} alt="" />
                  <span className="pv-upnext-play">
                    <Play size={26} fill="currentColor" />
                  </span>
                </button>
                <div className="pv-upnext-meta">
                  <b>{epLabel(nextEpisode)}: Episode {nextEpisode.number}</b>
                  <span>{formatTime(duration)}</span>
                </div>
                <button className="pv-upnext-btn" onClick={goNext} type="button">
                  <Play size={18} fill="currentColor" /> Next Episode
                </button>
              </div>
            )}
          </div>
        )}

        {/* Netflix: paused episode strip across the bottom */}
        {paused && showUi && !showEpisodes && upcoming.length > 0 && (
          <div className="pv-strip">
            <button className="pv-strip-ep current" onClick={() => setShowEpisodes(true)} type="button">
              <img src={getPosterUrl(playing)} alt="" />
              <span className="pv-strip-code">{epLabel(selectedEpisode)}</span>
              <span className="pv-strip-name">Episode {selectedEpisode.number}</span>
            </button>
            {upcoming.map((episode) => (
              <button
                key={`${seasonOf(episode)}-${episode.number}`}
                className="pv-strip-ep"
                onClick={() => switchEpisode(episode)}
                type="button"
              >
                <img src={getPosterUrl(playing)} alt="" />
                <span className="pv-strip-code">{epLabel(episode)}</span>
                <span className="pv-strip-name">Episode {episode.number}</span>
              </button>
            ))}
            <button className="pv-strip-ep pv-strip-more" onClick={() => setShowEpisodes(true)} type="button">
              <span className="pv-strip-more-icon">
                <List size={22} />
              </span>
              <span className="pv-strip-name">Episodes</span>
            </button>
          </div>
        )}

        {nextCountdown !== null && nextEpisode && !uiHidden && (
          <div className="nf-next">
            <div className="nf-next-card">
              <img src={getPosterUrl(playing)} alt="" />
              <div className="nf-next-meta">
                <span>Up Next in {nextCountdown}…</span>
                <b>
                  {epLabel(nextEpisode)}: Episode {nextEpisode.number}
                </b>
              </div>
              <button className="nf-circle" onClick={goNext} title="Play next now" type="button" aria-label="Play next now">
                <Play size={16} fill="currentColor" />
              </button>
              <button
                className="nf-circle"
                onClick={() => setNextCancelled(true)}
                title="Stay on this episode"
                type="button"
                aria-label="Stay on this episode"
              >
                <X size={16} />
              </button>
            </div>
          </div>
        )}

        {showEpisodes && (
          <aside className="nf-episodes">
            <div className="nf-episodes-head">
              <div>
                <strong>{playing.title}</strong>
                <span>{episodes.length} episodes</span>
              </div>
              <button className="nf-icon" onClick={() => setShowEpisodes(false)} type="button" aria-label="Close episodes">
                <X size={18} />
              </button>
            </div>

            <div className="nf-episodes-list">
              {seasons.map((season) => (
                <div key={season}>
                  {seasons.length > 1 && <div className="nf-episodes-season">Season {season}</div>}
                  {episodes
                    .filter((e) => seasonOf(e) === season)
                    .map((episode) => {
                      const active = episode.number === selectedEpisode.number;
                      return (
                        <button
                          key={`${season}-${episode.number}`}
                          className={`nf-ep${active ? " active" : ""}`}
                          onClick={() => {
                            if (!active) switchEpisode(episode);
                            setShowEpisodes(false);
                          }}
                          type="button"
                        >
                          <b>{episode.number}</b>
                          <span>Episode {episode.number}</span>
                          {active && <Play size={13} fill="currentColor" />}
                        </button>
                      );
                    })}
                </div>
              ))}
            </div>
          </aside>
        )}

        <div className="nf-bottom">
          <div className="nf-seek-row">
            <div
              ref={seekRef}
              className={`nf-seek${scrubbing ? " scrub" : ""}`}
              onPointerDown={onSeekDown}
              onPointerMove={onSeekMove}
              onPointerUp={(e) => commitSeek(e, false)}
              onPointerCancel={(e) => commitSeek(e, true)}
              onPointerLeave={() => {
                if (!scrubbingRef.current) setHover(null);
              }}
              role="slider"
              aria-label="Seek"
              aria-valuemin={0}
              aria-valuemax={Math.round(duration)}
              aria-valuenow={Math.round(shownTime)}
            >
              {hover && sprite && !videoError && (
                <div className="nf-preview" style={{ left: previewLeft }}>
                  <SeekPreview sprite={sprite} time={scrubbing ? scrubTime : hover.t} />
                  <span>{formatTime(scrubbing ? scrubTime : hover.t)}</span>
                </div>
              )}
              {hover && !sprite && !videoError && (
                <div className="nf-preview" style={{ left: previewLeft }}>
                  <span>{formatTime(scrubbing ? scrubTime : hover.t)}</span>
                </div>
              )}

              <div className="nf-seek-track">
                <div className="nf-seek-fill" style={{ width: `${progressPct}%` }} />
                <div className="nf-seek-thumb" style={{ left: `${progressPct}%` }} />
              </div>
            </div>

            <span className="nf-time">{formatTime(shownTime)} / {formatTime(duration)}</span>
          </div>

          <div className="nf-row-ctl">
            <div className="nf-group">
              <button className="nf-icon" onClick={togglePlay} title="Play / Pause (Space)" type="button" aria-label="Play or pause">
                {isPlaying ? <Pause size={30} fill="currentColor" /> : <Play size={30} fill="currentColor" />}
              </button>

              <button className="nf-icon" onClick={() => skip(-10)} title="Back 10s (←)" type="button" aria-label="Back 10 seconds">
                <RotateCcw size={26} />
                <span className="num">10</span>
              </button>

              <button className="nf-icon" onClick={() => skip(10)} title="Forward 10s (→)" type="button" aria-label="Forward 10 seconds">
                <RotateCw size={26} />
                <span className="num">10</span>
              </button>

              <div className="nf-volume">
                <button className="nf-icon" onClick={() => setMuted((v) => !v)} title="Mute (M)" type="button" aria-label="Mute">
                  {muted || volume === 0 ? <VolumeX size={26} /> : <Volume2 size={26} />}
                </button>
                <input
                  className="nf-vol"
                  type="range"
                  min="0"
                  max="1"
                  step=".01"
                  value={muted ? 0 : volume}
                  onChange={(event) => {
                    const value = Number(event.target.value);
                    setVolume(value);
                    setMuted(value === 0);
                  }}
                  style={{ ["--p" as string]: `${(muted ? 0 : volume) * 100}%` } as CSSProperties}
                  aria-label="Volume"
                />
              </div>
            </div>

            <div className="nf-ctl-title">
              <strong>{playing.title}</strong>
              <span>{epLabel(selectedEpisode)}: Episode {selectedEpisode.number}</span>
            </div>

            <div className="nf-group">
              {canGoNext && (
                <button className="nf-icon" onClick={goNext} title="Next episode (N)" type="button" aria-label="Next episode">
                  <SkipForward size={26} fill="currentColor" />
                </button>
              )}

              <button
                className="nf-icon"
                onClick={() => setShowEpisodes((v) => !v)}
                title="Episodes"
                type="button"
                aria-label="Episodes"
              >
                <List size={26} />
              </button>

              <button
                className="nf-icon nf-speed"
                onClick={() => setRate((r) => SPEEDS[(Math.max(0, SPEEDS.indexOf(r)) + 1) % SPEEDS.length])}
                title="Playback speed"
                type="button"
                aria-label="Playback speed"
              >
                <Gauge size={24} />
                <small>{rate}×</small>
              </button>

              <button
                className={`nf-icon${captions ? " on" : ""}`}
                onClick={() => setCaptions((v) => !v)}
                title="Subtitles (C)"
                type="button"
                aria-label="Subtitles"
              >
                <Captions size={26} />
              </button>

              <button
                className="nf-icon"
                onClick={() => void toggleFullscreen()}
                title="Fullscreen (F)"
                type="button"
                aria-label="Fullscreen"
              >
                {fullscreen ? <Minimize2 size={26} /> : <Maximize2 size={26} />}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

/* ================================================================== */
/*  Title card                                                        */
/* ================================================================== */

type CardProps = {
  item: CatalogEntry;
  bookmarked: boolean;
  progress?: number;
  resumeEpisode?: number;
  onPlay: (item: CatalogEntry, episode?: number) => void;
  onToggle: (id: string) => void;
  onInfo: (item: CatalogEntry) => void;
};

const TitleCard = memo(function TitleCard({ item, bookmarked, progress, resumeEpisode, onPlay, onToggle, onInfo }: CardProps) {
  const episodes = getEpisodes(item);
  const count = episodes.length;
  const hasVideo = count > 0;
  const [preview, setPreview] = useState(false);
  const hoverTimer = useRef<number | null>(null);

  useEffect(
    () => () => {
      if (hoverTimer.current) window.clearTimeout(hoverTimer.current);
    },
    []
  );

  const beginPreview = () => {
    if (!hasVideo || !window.matchMedia("(hover: hover)").matches) return;
    if (hoverTimer.current) window.clearTimeout(hoverTimer.current);
    hoverTimer.current = window.setTimeout(() => setPreview(true), 700);
  };

  const endPreview = () => {
    if (hoverTimer.current) window.clearTimeout(hoverTimer.current);
    hoverTimer.current = null;
    setPreview(false);
  };

  return (
    <article className="nf-card" onMouseEnter={beginPreview} onMouseLeave={endPreview}>
      <button
        className="nf-card-poster"
        disabled={!hasVideo}
        onClick={() => onPlay(item, resumeEpisode)}
        aria-label={`Play ${item.title}`}
        type="button"
      >
        <img src={getPosterUrl(item)} alt="" loading="lazy" decoding="async" />

        {preview && hasVideo && (
          <video src={getVideoUrl(item, episodes[0])} autoPlay muted loop playsInline preload="none" aria-hidden="true" />
        )}

        {!hasVideo && (
          <span className="nf-badge">
            <Sparkles size={10} /> Generating
          </span>
        )}

        <span className="nf-card-title">{item.title}</span>

        {progress !== undefined && (
          <span className="nf-progress">
            <i style={{ width: `${progress}%` }} />
          </span>
        )}
      </button>

      <div className="nf-pop">
        <div className="nf-pop-actions">
          <button className="nf-circle fill" disabled={!hasVideo} onClick={() => onPlay(item, resumeEpisode)} title="Play" type="button">
            <Play size={16} fill="currentColor" />
          </button>
          <button
            className="nf-circle"
            onClick={() => onToggle(item.id)}
            title={bookmarked ? "Remove from My List" : "Add to My List"}
            type="button"
          >
            {bookmarked ? <Check size={16} /> : <Plus size={16} />}
          </button>
          <button className="nf-circle end" onClick={() => onInfo(item)} title="More info" type="button">
            <ChevronDown size={18} />
          </button>
        </div>

        <div className="nf-meta">
          <span className="match">{matchScore(item.id)}% Match</span>
          <span className="maturity">TV-14</span>
          <span className="nf-pill">{item.format ?? "TV"}</span>
          <span>
            {count} {count === 1 ? "ep" : "eps"}
          </span>
        </div>

        {(item.genres ?? []).length > 0 && (
          <div className="nf-pop-genres">
            {(item.genres ?? []).slice(0, 3).map((genre) => (
              <span key={genre}>{genre}</span>
            ))}
          </div>
        )}
      </div>
    </article>
  );
});

/* ================================================================== */
/*  Row                                                               */
/* ================================================================== */

function Row({
  title,
  items,
  ranked,
  renderCard,
}: {
  title: string;
  items: CatalogEntry[];
  ranked?: boolean;
  renderCard: (item: CatalogEntry) => ReactElement;
}) {
  const trackRef = useRef<HTMLDivElement | null>(null);

  if (items.length === 0) return null;

  const scrollRow = (direction: number) => {
    const el = trackRef.current;
    if (el) el.scrollBy({ left: direction * el.clientWidth * 0.85, behavior: "smooth" });
  };

  return (
    <section className="nf-row">
      <h3>{title}</h3>

      <div className="nf-row-wrap">
        <button className="nf-arrow left" onClick={() => scrollRow(-1)} aria-label="Scroll left" type="button">
          <ChevronLeft size={34} />
        </button>

        <div className="nf-track" ref={trackRef}>
          {items.map((item, index) =>
            ranked ? (
              <div className="nf-rank" key={item.id}>
                <span className="nf-rank-num" aria-hidden>
                  {index + 1}
                </span>
                {renderCard(item)}
              </div>
            ) : (
              renderCard(item)
            )
          )}
        </div>

        <button className="nf-arrow right" onClick={() => scrollRow(1)} aria-label="Scroll right" type="button">
          <ChevronRight size={34} />
        </button>
      </div>
    </section>
  );
}

/* ================================================================== */
/*  Detail modal                                                      */
/* ================================================================== */

function DetailModal({
  item,
  bookmarked,
  progress,
  onPlay,
  onToggle,
  onClose,
}: {
  item: CatalogEntry;
  bookmarked: boolean;
  progress: Record<string, LocalProgress>;
  onPlay: (item: CatalogEntry, episode?: number) => void;
  onToggle: (id: string) => void;
  onClose: () => void;
}) {
  const episodes = getEpisodes(item);
  const seasons = seasonsOf(item);
  const [season, setSeason] = useState(seasons[0] ?? 1);
  const saved = progress[item.id];
  const poster = getPosterUrl(item);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      className="nf-modal-back"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="nf-modal" role="dialog" aria-label={item.title}>
        <button className="nf-modal-close" onClick={onClose} aria-label="Close" type="button">
          <X size={20} />
        </button>

        <div className="nf-modal-hero" style={{ backgroundImage: `url("${poster}")` }}>
          <div className="nf-modal-hero-shade" />
          <img src={poster} alt="" />
          <div className="content">
            <h2>{item.title}</h2>
            <div className="nf-modal-actions">
              <button
                className="nf-btn white big"
                disabled={episodes.length === 0}
                onClick={() => onPlay(item, saved?.episode)}
                type="button"
              >
                <Play size={20} fill="currentColor" />
                {saved ? "Resume" : "Play"}
              </button>
              <button
                className="nf-circle lg"
                onClick={() => onToggle(item.id)}
                title={bookmarked ? "Remove from My List" : "Add to My List"}
                type="button"
              >
                {bookmarked ? <Check size={20} /> : <Plus size={20} />}
              </button>
              {item.rating === "liked" && (
                <span className="nf-circle lg on" title="You liked this">
                  <ThumbsUp size={18} />
                </span>
              )}
            </div>
          </div>
        </div>

        <div className="nf-modal-body">
          <div className="nf-meta">
            <span className="match">{matchScore(item.id)}% Match</span>
            {item.year && <span>{item.year}</span>}
            <span className="maturity">TV-14</span>
            <span>
              {episodes.length} {episodes.length === 1 ? "Episode" : "Episodes"}
            </span>
            <span className="nf-pill">HD</span>
          </div>

          <p className="synopsis">{item.synopsis ?? "No synopsis available yet."}</p>

          {(item.genres ?? []).length > 0 && (
            <p className="genres">
              <span>Genres:</span> {(item.genres ?? []).join(", ")}
            </p>
          )}

          {episodes.length > 0 && (
            <>
              <div className="nf-eps-head">
                <h4>Episodes</h4>
                {seasons.length > 1 && (
                  <select value={season} onChange={(e) => setSeason(Number(e.target.value))} aria-label="Season">
                    {seasons.map((s) => (
                      <option key={s} value={s}>
                        Season {s}
                      </option>
                    ))}
                  </select>
                )}
              </div>

              <div className="nf-ep-list">
                {episodes
                  .filter((e) => seasonOf(e) === season)
                  .map((episode) => {
                    const pct =
                      saved && saved.episode === episode.number && saved.duration > 0
                        ? clamp((saved.position / saved.duration) * 100, 0, 100)
                        : 0;

                    return (
                      <button
                        key={`${season}-${episode.number}`}
                        className="nf-ep-row"
                        onClick={() => onPlay(item, episode.number)}
                        type="button"
                      >
                        <span className="n">{episode.number}</span>
                        <img src={poster} alt="" loading="lazy" />
                        <span className="t">
                          <b>Episode {episode.number}</b>
                          <small>{item.synopsis ?? ""}</small>
                        </span>
                        <Play size={18} fill="currentColor" />
                        {pct > 1 && (
                          <span className="bar">
                            <i style={{ width: `${pct}%` }} />
                          </span>
                        )}
                      </button>
                    );
                  })}
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

/* ================================================================== */
/*  App                                                               */
/* ================================================================== */

function App() {
  const [catalog, setCatalog] = useState<CatalogEntry[]>(() => normalizeCatalog(catalogData));
  const [initialLoading, setInitialLoading] = useState(true);
  const [libraryCount, setLibraryCount] = useState(0);

  const [view, setView] = useState<View>("home");
  const [formatFilter, setFormatFilter] = useState<FormatFilter>("all");
  const [genreFilter, setGenreFilter] = useState("all");
  const [sortMode, setSortMode] = useState<SortMode>("title");

  const [searchOpen, setSearchOpen] = useState(false);
  const [searchFocused, setSearchFocused] = useState(false);
  const [searchText, setSearchText] = useState("");
  const [submitted, setSubmitted] = useState("");
  const [activeSuggest, setActiveSuggest] = useState(-1);
  const [remote, setRemote] = useState<CatalogEntry[]>([]);

  const [notes, setNotes] = useState<Note[]>([]);
  const [bellOpen, setBellOpen] = useState(false);
  const [heroPaused, setHeroPaused] = useState(false);
  const [outgoing, setOutgoing] = useState<CatalogEntry | null>(null);

  const [bookmarks, setBookmarks] = useState<string[]>(loadBookmarks);
  const [progress, setProgress] = useState<Record<string, LocalProgress>>(loadContinue);

  const [playing, setPlaying] = useState<CatalogEntry | null>(null);
  const [selectedEpisode, setSelectedEpisode] = useState<Episode | null>(null);
  const [historyBusy, setHistoryBusy] = useState(false);
  const [historyError, setHistoryError] = useState("");
  const [generationRunning, setGenerationRunning] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [detailItem, setDetailItem] = useState<CatalogEntry | null>(null);
  const [scrolled, setScrolled] = useState(false);
  const [heroIndex, setHeroIndex] = useState(0);
  const [showIntro, setShowIntro] = useState(true);

  const toastTimer = useRef<number | null>(null);
  const generationPollRef = useRef<number | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const bellRef = useRef<HTMLDivElement | null>(null);
  const noteId = useRef(0);
  const heroRef = useRef<CatalogEntry | null>(null);
  const prevHeroRef = useRef<CatalogEntry | null>(null);

  const showToast = useCallback((message: string) => {
    setToast(message);
    if (toastTimer.current) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 3200);
  }, []);

  useEffect(
    () => () => {
      if (toastTimer.current) window.clearTimeout(toastTimer.current);
      if (generationPollRef.current) window.clearInterval(generationPollRef.current);
    },
    []
  );

  const finishIntro = useCallback(() => setShowIntro(false), []);

  const byId = useMemo(() => new Map(catalog.map((item) => [item.id, item])), [catalog]);
  const detail = detailItem ? byId.get(detailItem.id) ?? detailItem : null;

  const locked = Boolean(playing || detail || showIntro);
  useEffect(() => {
    document.body.style.overflow = locked ? "hidden" : "";
    return () => {
      document.body.style.overflow = "";
    };
  }, [locked]);

  const playable = useMemo(() => catalog.filter((item) => getEpisodes(item).length > 0), [catalog]);
  const catalogSig = useMemo(() => catalog.map((item) => item.id).join("|"), [catalog]);

  const trendingIds = useMemo(
    () => shuffle(playable).slice(0, 12).map((item) => item.id),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [catalogSig]
  );

  const trending = useMemo(
    () => trendingIds.map((id) => byId.get(id)).filter((item): item is CatalogEntry => Boolean(item)),
    [trendingIds, byId]
  );

  const heroItems = useMemo(() => trending.slice(0, 6), [trending]);
  const heroSig = heroItems.map((item) => item.id).join("|");
  const hero = heroItems[heroIndex] ?? heroItems[0] ?? null;

  useEffect(() => {
    setHeroIndex(0);
  }, [heroSig]);

  heroRef.current = hero;
  const heroId = hero?.id;

  useEffect(() => {
    const prev = prevHeroRef.current;
    const next = heroRef.current;
    prevHeroRef.current = next;
    if (!prev || !next || prev.id === next.id) return;

    setOutgoing(prev);
    const timer = window.setTimeout(() => setOutgoing(null), 900);
    return () => window.clearTimeout(timer);
  }, [heroId]);

  const allGenres = useMemo(() => {
    const counts = new Map<string, number>();
    catalog.forEach((item) => (item.genres ?? []).forEach((g) => counts.set(g, (counts.get(g) ?? 0) + 1)));
    return [...counts.entries()].sort((a, b) => b[1] - a[1]).map(([g]) => g);
  }, [catalog]);

  const typed = searchText.trim();
  const query = submitted;
  const searching = submitted.length > 0;

  /* remote search (debounced) */
  useEffect(() => {
    if (typed.length < 2) {
      setRemote([]);
      return;
    }
    const controller = new AbortController();
    const timer = window.setTimeout(async () => {
      try {
        const response = await fetch(`/api/search?q=${encodeURIComponent(typed)}`, { signal: controller.signal });
        const data = response.ok ? await response.json() : [];
        setRemote(normalizeCatalog(data));
      } catch {
        /* aborted, or endpoint missing */
      }
    }, 300);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [typed]);

  const matchTitles = useCallback(
    (q: string) => {
      const needle = q.toLowerCase();
      const local = catalog.filter(
        (item) => item.title.toLowerCase().includes(needle) || (item.genres ?? []).some((g) => g.toLowerCase().includes(needle))
      );
      if (q !== typed) return local;
      const ids = new Set(local.map((i) => i.id));
      return [...local, ...remote.filter((r) => !ids.has(r.id))];
    },
    [catalog, remote, typed]
  );

  const suggestions = useMemo(() => (typed ? matchTitles(typed).slice(0, 6) : []), [typed, matchTitles]);
  const searchResults = useMemo(() => (searching ? matchTitles(submitted) : []), [searching, submitted, matchTitles]);

  const filteredCatalog = useMemo(() => {
    const list = catalog.filter((item) => {
      if (view === "list" && !bookmarks.includes(item.id)) return false;
      if (formatFilter !== "all" && item.format !== formatFilter) return false;
      if (genreFilter !== "all" && !(item.genres ?? []).includes(genreFilter)) return false;
      return true;
    });

    return list.sort((a, b) => {
      if (sortMode === "title") return a.title.localeCompare(b.title);
      if (sortMode === "year") return (b.year ?? 0) - (a.year ?? 0);
      return getEpisodes(b).length - getEpisodes(a).length;
    });
  }, [catalog, view, bookmarks, formatFilter, genreFilter, sortMode]);

  const rows = useMemo(() => {
    const myList = catalog.filter((item) => bookmarks.includes(item.id));
    const liked = playable.filter((item) => item.rating === "liked");

    const continueItems = Object.entries(progress)
      .filter(([, s]) => s.position > 5 && (s.duration <= 0 || s.position < s.duration - 10))
      .sort((a, b) => b[1].at - a[1].at)
      .map(([id]) => byId.get(id))
      .filter((item): item is CatalogEntry => Boolean(item) && getEpisodes(item as CatalogEntry).length > 0);

    const genreRows = allGenres
      .slice(0, 8)
      .map((genre) => ({
        name: genre,
        items: playable.filter((item) => (item.genres ?? []).includes(genre)).slice(0, 14),
      }))
      .filter((row) => row.items.length >= 4)
      .slice(0, 5);

    const newest = [...playable].sort((a, b) => (b.year ?? 0) - (a.year ?? 0)).slice(0, 14);
    const all = [...catalog].sort((a, b) => a.title.localeCompare(b.title));

    return { myList, liked, continueItems, genreRows, newest, all };
  }, [catalog, playable, bookmarks, progress, byId, allGenres]);

  const refreshCatalog = useCallback(async () => {
    try {
      const response = await fetch("/api/catalog");
      if (!response.ok) return;
      const data = normalizeCatalog(await response.json());
      setCatalog(data);
      setPlaying((current) => (current ? data.find((item) => item.id === current.id) ?? current : null));
    } catch (error) {
      console.error("Failed to refresh catalog:", error);
    }
  }, []);

  const refreshLibraryCount = useCallback(async () => {
    try {
      const response = await fetch("/api/library/count");
      if (!response.ok) return;
      setLibraryCount(((await response.json()) as { count: number }).count);
    } catch (error) {
      console.error("Failed to load library count:", error);
    }
  }, []);

  useEffect(() => {
    void refreshCatalog().finally(() => setInitialLoading(false));
    void refreshLibraryCount();

    const onScroll = () => setScrolled(window.scrollY > 24);
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, [refreshCatalog, refreshLibraryCount]);

  useEffect(() => {
    if (!playing) return;
    const episodes = getEpisodes(playing);
    if (selectedEpisode && !episodes.some((e) => e.number === selectedEpisode.number)) {
      setSelectedEpisode(episodes[0] ?? null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing]);

  const toggleBookmark = useCallback((id: string) => {
    setBookmarks((current) => {
      const next = current.includes(id) ? current.filter((v) => v !== id) : [...current, id];
      writeJSON(BOOKMARK_KEY, next);
      return next;
    });
  }, []);

  const openPlayer = useCallback((item: CatalogEntry, episodeNumber?: number) => {
    const episodes = getEpisodes(item);
    if (episodes.length === 0) return;

    const local = loadContinue()[item.id];
    const wanted = episodeNumber ?? local?.episode;
    const episode = episodes.find((v) => v.number === wanted) ?? episodes[0];

    setHistoryError("");
    setDetailItem(null);
    setPlaying(item);
    setSelectedEpisode(episode);
  }, []);

  const closePlayer = useCallback(() => {
    setPlaying(null);
    setSelectedEpisode(null);
    setHistoryError("");
    setProgress(loadContinue());
    if (document.fullscreenElement) void document.exitFullscreen().catch(() => {});
  }, []);

  const closeDetail = useCallback(() => setDetailItem(null), []);

  const updateRating = (id: string, rating: Rating) => {
    setCatalog((current) => current.map((item) => (item.id === id ? { ...item, rating } : item)));
    setPlaying((current) => (current && current.id === id ? { ...current, rating } : current));
  };

  const handleHistory = async (liked: boolean) => {
    if (!playing || !selectedEpisode || historyBusy) return;
    setHistoryBusy(true);
    setHistoryError("");

    try {
      const response = await fetch(liked ? "/api/history/like" : "/api/history/dislike", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id: playing.id, title: playing.title, episode: selectedEpisode.number }),
      });
      const body = await response.text();
      if (!response.ok) throw new Error(body || "History request failed");

      updateRating(playing.id, liked ? "liked" : "disliked");
      showToast(liked ? `Liked “${playing.title}”` : `Disliked “${playing.title}”`);
      await refreshCatalog();
    } catch (error) {
      console.error("History action failed:", error);
      setHistoryError(error instanceof Error ? error.message : "History action failed");
    } finally {
      setHistoryBusy(false);
    }
  };

  const generateMore = async () => {
    if (generationRunning) return;

    try {
      const response = await fetch("/api/generate", { method: "POST" });

      if (response.status === 409) {
        showToast("Generation is already running on the server");
        return;
      }
      if (!response.ok) throw new Error("Failed to start generation");

      setGenerationRunning(true);
      showToast("Generation started — building new titles…");
      pushNote("Generation started — building new titles.");

      if (generationPollRef.current) window.clearInterval(generationPollRef.current);

      const stop = () => {
        if (generationPollRef.current) window.clearInterval(generationPollRef.current);
        generationPollRef.current = null;
        setGenerationRunning(false);
      };

      generationPollRef.current = window.setInterval(async () => {
        try {
          const statusResponse = await fetch("/api/generate/status");
          if (!statusResponse.ok) return;
          const status = (await statusResponse.json()) as { running: boolean };

          if (!status.running) {
            stop();
            await refreshCatalog();
            await refreshLibraryCount();
            showToast("New titles added to your library");
            pushNote("New titles were added to your library.");
          }
        } catch (error) {
          console.error("Generation status failed:", error);
          stop();
        }
      }, 1500);
    } catch (error) {
      console.error("Generate More failed:", error);
      setGenerationRunning(false);
      showToast("Couldn’t start generation");
      pushNote("Couldn’t start generation. Check the server and try again.");
    }
  };

  const pushNote = useCallback((text: string) => {
    setNotes((current) => [{ id: ++noteId.current, text, at: Date.now(), unread: true }, ...current].slice(0, 8));
  }, []);

  const unreadCount = notes.filter((n) => n.unread).length;

  const toggleBell = () => {
    if (!bellOpen) setNotes((current) => current.map((n) => ({ ...n, unread: false })));
    setBellOpen(!bellOpen);
  };

  useEffect(() => {
    if (!bellOpen) return;
    const onDown = (event: MouseEvent) => {
      if (bellRef.current && !bellRef.current.contains(event.target as Node)) setBellOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setBellOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [bellOpen]);

  const goView = (next: View) => {
    setView(next);
    setFormatFilter("all");
    setGenreFilter("all");
    setSearchText("");
    setSubmitted("");
    setSearchOpen(false);
    window.scrollTo({ top: 0, behavior: "smooth" });
  };

  const openSearch = () => {
    setSearchOpen(true);
    window.setTimeout(() => searchInputRef.current?.focus(), 30);
  };

  const closeSearch = () => {
    setSearchText("");
    setSubmitted("");
    setActiveSuggest(-1);
    setSearchOpen(false);
    searchInputRef.current?.blur();
  };

  const submitSearch = () => {
    if (!typed) return;
    setSubmitted(typed);
    setActiveSuggest(-1);
    searchInputRef.current?.blur();
    window.scrollTo({ top: 0, behavior: "smooth" });
  };

  const pickSuggestion = (item: CatalogEntry) => {
    setDetailItem(item);
    closeSearch();
  };

  /* "/" focuses search, like on Netflix-style sites */
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey) return;
      const target = event.target;
      if (target instanceof HTMLInputElement || target instanceof HTMLSelectElement || target instanceof HTMLTextAreaElement) return;
      if (playing || detail || showIntro) return;
      event.preventDefault();
      openSearch();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing, detail, showIntro]);

  const renderCard = useCallback(
    (item: CatalogEntry) => {
      const saved = progress[item.id];
      const pct =
        saved && saved.duration > 0 && saved.position > 5
          ? clamp((saved.position / saved.duration) * 100, 0, 100)
          : undefined;

      return (
        <TitleCard
          key={item.id}
          item={item}
          bookmarked={bookmarks.includes(item.id)}
          progress={pct}
          resumeEpisode={saved?.episode}
          onPlay={openPlayer}
          onToggle={toggleBookmark}
          onInfo={setDetailItem}
        />
      );
    },
    [bookmarks, progress, openPlayer, toggleBookmark]
  );

  const browsing = view !== "home";
  const gridMode = searching || browsing || formatFilter !== "all" || genreFilter !== "all";
  const showHero = view === "home" && !gridMode && !initialLoading && Boolean(hero);
  const libraryPct = Math.min(100, (libraryCount / LIBRARY_LIMIT) * 100);
  const playingEpisodes = playing ? getEpisodes(playing) : [];
  const gridItems = searching ? searchResults : filteredCatalog;

  return (
    <div className="nf-app">
      {showIntro && <Intro onDone={finishIntro} />}

      {/* ---------------- navigation ---------------- */}
      <header className={`nf-nav${scrolled || gridMode ? " solid" : ""}`}>
        <div className="nf-nav-left">
          <button className="nf-logo" onClick={() => goView("home")} aria-label={`${BRAND} home`} type="button">
            {BRAND}
          </button>

          <nav>
            <button className={view === "home" && !searching ? "active" : ""} onClick={() => goView("home")} type="button">
              Home
            </button>
            <button className={view === "browse" ? "active" : ""} onClick={() => goView("browse")} type="button">
              TV Shows
            </button>
            <button className={view === "list" ? "active" : ""} onClick={() => goView("list")} type="button">
              My List
            </button>
            <button className={view === "add" ? "active" : ""} onClick={() => goView("add")} type="button">
              Add Anime
            </button>
            <button className={view === "manga" ? "active" : ""} onClick={() => goView("manga")} type="button">
              Manga
            </button>
          </nav>
        </div>

        <div className="nf-nav-right">
          <div className={`nf-search${searchOpen ? " open" : ""}`}>
            <button
              className="nf-nav-btn"
              onClick={() => {
                if (searchOpen && typed) submitSearch();
                else if (searchOpen) closeSearch();
                else openSearch();
              }}
              aria-label="Search"
              type="button"
            >
              <Search size={20} />
            </button>

            <input
              ref={searchInputRef}
              value={searchText}
              onChange={(event) => {
                setSearchText(event.target.value);
                setActiveSuggest(-1);
                if (!event.target.value.trim()) setSubmitted("");
              }}
              onFocus={() => setSearchFocused(true)}
              onBlur={() => {
                setSearchFocused(false);
                if (!searchText) setSearchOpen(false);
              }}
              onKeyDown={(event) => {
                if (event.key === "ArrowDown") {
                  event.preventDefault();
                  setActiveSuggest((i) => Math.min(i + 1, suggestions.length - 1));
                } else if (event.key === "ArrowUp") {
                  event.preventDefault();
                  setActiveSuggest((i) => Math.max(i - 1, -1));
                } else if (event.key === "Enter") {
                  const chosen = suggestions[activeSuggest];
                  if (chosen) pickSuggestion(chosen);
                  else submitSearch();
                } else if (event.key === "Escape") {
                  closeSearch();
                }
              }}
              placeholder="Titles, genres"
              aria-label="Search titles"
              autoComplete="off"
            />

            {searchOpen && searchText && (
              <button className="nf-nav-btn" onClick={closeSearch} aria-label="Clear search" type="button">
                <X size={16} />
              </button>
            )}

            {searchOpen && searchFocused && suggestions.length > 0 && (
              <div className="nf-suggest" role="listbox">
                {suggestions.map((item, index) => (
                  <button
                    key={item.id}
                    role="option"
                    aria-selected={index === activeSuggest}
                    className={index === activeSuggest ? "active" : ""}
                    onMouseDown={(event) => event.preventDefault()}
                    onMouseEnter={() => setActiveSuggest(index)}
                    onClick={() => pickSuggestion(item)}
                    type="button"
                  >
                    <Search size={15} />
                    <span className="t">
                      <strong>{item.title}</strong>
                      <small>{[item.format, item.year].filter(Boolean).join(" · ") || "Anime"}</small>
                    </span>
                  </button>
                ))}
                <button
                  className="all"
                  onMouseDown={(event) => event.preventDefault()}
                  onClick={submitSearch}
                  type="button"
                >
                  Explore titles related to “{typed}”
                </button>
              </div>
            )}
          </div>

          <button
            className={`nf-gen${generationRunning ? " busy" : ""}`}
            onClick={generateMore}
            disabled={generationRunning}
            title="Build new titles for your library"
            type="button"
          >
            <Sparkles size={16} className={generationRunning ? "spin" : undefined} />
            <span className="txt">{generationRunning ? "Generating…" : "Generate More"}</span>
            <span className="cap" title="Library capacity">
              {libraryCount}
              <em>/{LIBRARY_LIMIT}</em>
            </span>
            <span className="bar">
              <i style={{ width: `${libraryPct}%` }} />
            </span>
          </button>

          <div className="nf-bell-wrap" ref={bellRef}>
            <button
              className={`nf-nav-btn nf-bell${generationRunning ? " ring" : ""}`}
              onClick={toggleBell}
              aria-label="Notifications"
              aria-expanded={bellOpen}
              type="button"
            >
              <Bell size={20} />
              {generationRunning ? (
                <span className="dot live" />
              ) : (
                unreadCount > 0 && <span className="dot">{unreadCount}</span>
              )}
            </button>

            {bellOpen && (
              <div className="nf-notes">
                <div className="nf-notes-head">Notifications</div>

                {generationRunning && (
                  <div className="nf-note live">
                    <Sparkles size={16} className="spin" />
                    <span>Building new titles for your library…</span>
                  </div>
                )}

                {notes.length === 0 && !generationRunning ? (
                  <div className="nf-notes-empty">You’re all caught up.</div>
                ) : (
                  notes.map((note) => (
                    <div className="nf-note" key={note.id}>
                      <Check size={16} />
                      <span>
                        {note.text}
                        <small>{timeAgo(note.at)}</small>
                      </span>
                    </div>
                  ))
                )}
              </div>
            )}
          </div>

          <div className="nf-avatar" title="Profile">
            A
          </div>
        </div>
      </header>

      {/* ---------------- billboard ---------------- */}
      {showHero && hero && (
        <section
          className="nf-hero"
          onMouseEnter={() => setHeroPaused(true)}
          onMouseLeave={() => setHeroPaused(false)}
        >
          <div className="nf-hero-stage" aria-hidden>
            {outgoing && outgoing.id !== hero.id && (
              <div
                className="nf-hero-bg out"
                key={`out-${outgoing.id}`}
                style={{ backgroundImage: `url("${getPosterUrl(outgoing)}")` }}
              />
            )}
            <div className="nf-hero-bg in" key={`bg-${hero.id}`} style={{ backgroundImage: `url("${getPosterUrl(hero)}")` }} />
          </div>
          <div className="nf-hero-shade" />

          <div className="nf-hero-art" aria-hidden>
            <div className="nf-hero-glow" />
            {heroItems.length >= 3 && (
              <>
                <img
                  className="nf-hero-side a"
                  src={getPosterUrl(heroItems[(heroIndex + 1) % heroItems.length])}
                  alt=""
                  decoding="async"
                />
                <img
                  className="nf-hero-side b"
                  src={getPosterUrl(heroItems[(heroIndex - 1 + heroItems.length) % heroItems.length])}
                  alt=""
                  decoding="async"
                />
              </>
            )}
            <img className="nf-hero-poster" key={`p-${hero.id}`} src={getPosterUrl(hero)} alt="" decoding="async" />
          </div>

          <div className="nf-hero-content" key={`c-${hero.id}`}>
            <div className="nf-hero-tag">
              <span className="nf-top10">
                TOP
                <b>10</b>
              </span>
              <span>#{heroIndex + 1} in Anime Today</span>
            </div>

            <h1 title={hero.title}>{hero.title}</h1>

            <div className="nf-meta">
              <span className="match">{matchScore(hero.id)}% Match</span>
              <span>{hero.year ?? "—"}</span>
              <span className="maturity">TV-14</span>
              <span>{getEpisodes(hero).length} Episodes</span>
              <span className="nf-pill">HD</span>
            </div>

            {(hero.genres ?? []).length > 0 && (
              <div className="nf-hero-genres">
                {(hero.genres ?? []).slice(0, 4).map((genre) => (
                  <span key={genre}>{genre}</span>
                ))}
              </div>
            )}

            {hero.synopsis && <p>{hero.synopsis}</p>}

            <div className="nf-hero-actions">
              <button className="nf-btn white big" onClick={() => openPlayer(hero)} type="button">
                <Play size={22} fill="currentColor" />
                {progress[hero.id] ? "Resume" : "Play"}
              </button>
              <button className="nf-btn grey big" onClick={() => setDetailItem(hero)} type="button">
                <Info size={22} /> More Info
              </button>
              <button
                className="nf-circle lg"
                onClick={() => toggleBookmark(hero.id)}
                title={bookmarks.includes(hero.id) ? "Remove from My List" : "Add to My List"}
                type="button"
              >
                {bookmarks.includes(hero.id) ? <Check size={20} /> : <Plus size={20} />}
              </button>
            </div>
          </div>

          <div className="nf-hero-rail">
            {heroItems.map((item, index) => (
              <button
                key={item.id}
                className={`nf-rail-tile${index === heroIndex ? " on" : ""}`}
                onClick={() => setHeroIndex(index)}
                aria-label={`Show ${item.title}`}
                title={item.title}
                type="button"
              >
                <img src={getPosterUrl(item)} alt="" loading="lazy" decoding="async" />
                {index === heroIndex && (
                  <span
                    key={`fill-${heroIndex}-${hero.id}`}
                    className={`fill${heroPaused || playing || detail ? " paused" : ""}`}
                    onAnimationEnd={(event) => {
                      if (event.target === event.currentTarget && heroItems.length > 1) {
                        setHeroIndex((i) => (i + 1) % heroItems.length);
                      }
                    }}
                  />
                )}
              </button>
            ))}
          </div>
        </section>
      )}

      {/* ---------------- content ---------------- */}
      <main className={`nf-main${showHero ? " over" : " padded"}`}>
        {view === "add" ? (
          <AnimeRequest />
        ) : view === "manga" ? (
          <section className="manga-coming" aria-labelledby="manga-title">
            <span className="manga-mark" aria-hidden="true">漫</span>
            <p className="ar-kicker">ANIFLEX LIBRARY</p>
            <h1 id="manga-title">Manga</h1>
            <span className="manga-rule" />
            <p>Coming very soon</p>
          </section>
        ) : gridMode && (
          <section className="nf-browse-head">
            <div>
              <h1>{searching ? `Explore titles related to “${query}”` : view === "list" ? "My List" : "TV Shows"}</h1>
              <p>
                {gridItems.length} {gridItems.length === 1 ? "title" : "titles"}
                {!searching && formatFilter !== "all" ? ` · ${formatFilter}` : ""}
                {!searching && genreFilter !== "all" ? ` · ${genreFilter}` : ""}
              </p>
            </div>

            {!searching && (
              <div className="nf-tools">
                <div className="nf-chips">
                  {(["all", "TV", "Movie", "OVA", "ONA"] as FormatFilter[]).map((format) => (
                    <button
                      key={format}
                      className={formatFilter === format ? "active" : ""}
                      onClick={() => setFormatFilter(format)}
                      type="button"
                    >
                      {format === "all" ? "All" : format}
                    </button>
                  ))}
                </div>

                {allGenres.length > 0 && (
                  <select value={genreFilter} onChange={(e) => setGenreFilter(e.target.value)} aria-label="Genre">
                    <option value="all">All genres</option>
                    {allGenres.map((genre) => (
                      <option key={genre} value={genre}>
                        {genre}
                      </option>
                    ))}
                  </select>
                )}

                <select value={sortMode} onChange={(e) => setSortMode(e.target.value as SortMode)} aria-label="Sort">
                  <option value="title">Suggested for you</option>
                  <option value="year">Year Released</option>
                  <option value="episodes">Most episodes</option>
                </select>
              </div>
            )}
          </section>
        )}

        {view === "add" || view === "manga" ? null : initialLoading ? (
          <div className="nf-grid">
            {Array.from({ length: 12 }).map((_, index) => (
              <div className="nf-skel" key={index}>
                <div className="p" />
                <div className="l" />
              </div>
            ))}
          </div>
        ) : gridMode ? (
          gridItems.length === 0 ? (
            <div className="nf-empty">
              <Search size={42} />
              <h2>
                {searching
                  ? `No results for “${query}”`
                  : view === "list"
                    ? "Your list is empty"
                    : "Nothing matches these filters"}
              </h2>
              <p>
                {searching
                  ? "Check the spelling, or try a genre."
                  : view === "list"
                    ? "Hover a title and select + to save it here."
                    : "Try a different format or genre."}
              </p>
              {!searching && (
                <button
                  className="nf-btn white"
                  onClick={() => {
                    setFormatFilter("all");
                    setGenreFilter("all");
                    if (view === "list") goView("browse");
                  }}
                  type="button"
                >
                  {view === "list" ? "Browse titles" : "Clear filters"}
                </button>
              )}
            </div>
          ) : (
            <div className="nf-grid">{gridItems.map(renderCard)}</div>
          )
        ) : (
          <>
            <Row title="Continue Watching" items={rows.continueItems} renderCard={renderCard} />
            <Row title={`Top 10 on ${BRAND} Today`} items={trending.slice(0, 10)} ranked renderCard={renderCard} />
            <Row title="My List" items={rows.myList} renderCard={renderCard} />
            <Row title="Because You Liked" items={rows.liked} renderCard={renderCard} />
            <Row title="New Releases" items={rows.newest} renderCard={renderCard} />
            {rows.genreRows.map((row) => (
              <Row key={row.name} title={row.name} items={row.items} renderCard={renderCard} />
            ))}
            {/*<Row title="All Anime" items={rows.all} renderCard={renderCard} />*/}
          </>
        )}
      </main>

      <footer className="nf-footer">
        <div className="links">
          <span>Audio Description</span>
          <span>Help Center</span>
          <span>Media Center</span>
          <span>Privacy</span>
          <span>Terms of Use</span>
          <span>Legal Notices</span>
          <span>Contact Us</span>
          <span>Cookie Preferences</span>
        </div>
        <div className="brand">{BRAND}</div>
        <p>{BRAND} — your personal media library. All titles are locally generated.</p>
      </footer>

      {/* ---------------- overlays ---------------- */}
      {detail && !playing && (
        <DetailModal
          key={detail.id}
          item={detail}
          bookmarked={bookmarks.includes(detail.id)}
          progress={progress}
          onPlay={openPlayer}
          onToggle={toggleBookmark}
          onClose={closeDetail}
        />
      )}

      {playing && selectedEpisode && (
        <Player
          playing={playing}
          selectedEpisode={selectedEpisode}
          episodes={playingEpisodes}
          historyBusy={historyBusy}
          historyError={historyError}
          onSelectEpisode={setSelectedEpisode}
          onHistory={handleHistory}
          onClose={closePlayer}
        />
      )}

      {toast && (
        <div className="nf-toast" role="status">
          {generationRunning ? <Sparkles size={15} /> : <Check size={15} />}
          {toast}
        </div>
      )}
    </div>
  );
}

/* ================================================================== */
/*  Error boundary: never show a blank page, show what went wrong     */
/* ================================================================== */

class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: { componentStack?: string | null }) {
    console.error("ANIFLEX crashed:", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;

    const box: CSSProperties = {
      minHeight: "100vh",
      display: "grid",
      placeContent: "center",
      justifyItems: "center",
      gap: 14,
      padding: 24,
      textAlign: "center",
      background: "#141414",
      color: "#fff",
      fontFamily: "Helvetica, Arial, sans-serif",
    };
    const btn: CSSProperties = {
      padding: "10px 20px",
      borderRadius: 4,
      border: 0,
      fontWeight: 700,
      cursor: "pointer",
      background: "#fff",
      color: "#000",
    };

    return (
      <div style={box}>
        <h2 style={{ margin: 0 }}>Something went wrong</h2>
        <pre style={{ maxWidth: 720, whiteSpace: "pre-wrap", color: "#ff8a8a", margin: 0 }}>
          {error.message || String(error)}
        </pre>
        <div style={{ display: "flex", gap: 10, flexWrap: "wrap", justifyContent: "center" }}>
          <button style={btn} onClick={() => this.setState({ error: null })} type="button">
            Try again
          </button>
          <button
            style={btn}
            onClick={() => {
              try {
                localStorage.removeItem(CONTINUE_KEY);
              } catch {
                /* ignore */
              }
              window.location.reload();
            }}
            type="button"
          >
            Clear watch progress &amp; reload
          </button>
        </div>
      </div>
    );
  }
}

export default function Root() {
  return (
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  );
}