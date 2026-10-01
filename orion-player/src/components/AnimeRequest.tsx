import { useEffect, useState } from "react";
import { Check, Download, LoaderCircle, Search } from "lucide-react";

type AniListAnime = {
  id: number;
  title: { romaji?: string | null; english?: string | null; native?: string | null };
  episodes?: number | null;
  format?: string | null;
  seasonYear?: number | null;
  description?: string | null;
  coverImage?: { extraLarge?: string | null; large?: string | null };
};

const displayTitle = (anime: AniListAnime) => anime.title.english || anime.title.romaji || anime.title.native || "Untitled anime";

export default function AnimeRequest() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<AniListAnime[]>([]);
  const [selected, setSelected] = useState<AniListAnime | null>(null);
  const [searching, setSearching] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    const value = query.trim();
    if (value.length < 2) {
      setResults([]);
      setSearching(false);
      return;
    }

    const controller = new AbortController();
    const timer = window.setTimeout(async () => {
      setSearching(true);
      setError("");
      try {
        const response = await fetch("/api/anime/search", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ query: value }),
          signal: controller.signal,
        });
        if (!response.ok) throw new Error(await response.text() || "AniList search failed");
        const data: unknown = await response.json();
        setResults(Array.isArray(data) ? data as AniListAnime[] : []);
      } catch (reason) {
        if (!controller.signal.aborted) {
          setError(reason instanceof Error ? reason.message : "AniList search failed");
          setResults([]);
        }
      } finally {
        if (!controller.signal.aborted) setSearching(false);
      }
    }, 350);

    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [query]);

  const chooseAnime = (anime: AniListAnime) => {
    setSelected(anime);
    setMessage("");
    setError("");
  };

  const startDownload = async (episode: number) => {
    if (!selected || busy) return;
    setBusy(true);
    setMessage("");
    setError("");
    try {
      const response = await fetch("/api/anime/download", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          title: displayTitle(selected),
          episode,
          total_episodes: selected.episodes ?? null,
          poster_url: selected.coverImage?.extraLarge || selected.coverImage?.large || null,
        }),
      });
      if (!response.ok) throw new Error(await response.text() || "Could not start download");
      setMessage(`Download has begun for ${displayTitle(selected)}, episode ${episode}.`);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Could not start download");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="ar-page" aria-labelledby="ar-title">
      <div className="ar-head">
        <div className="ar-kicker">PERSONAL DOWNLOADS</div>
        <h1 id="ar-title">Add an anime</h1>
        <p>Search AniList, choose a series, then download one episode.</p>
      </div>

      <div className="ar-layout">
        <div className="ar-search-column">
          <label className="ar-searchbox">
            <Search size={19} aria-hidden="true" />
            <input
              type="search"
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                setSelected(null);
                setMessage("");
              }}
              placeholder="Search anime titles"
              autoComplete="off"
            />
            {searching && <LoaderCircle className="ar-spinner" size={18} aria-label="Searching" />}
          </label>

          {error && <p className="ar-feedback error" role="alert">{error}</p>}
          {!error && query.trim().length >= 2 && !searching && results.length === 0 && (
            <p className="ar-feedback">No matching anime found.</p>
          )}
          <div className="ar-results" aria-live="polite">
            {results.map((anime) => (
              <button
                className={`ar-result${selected?.id === anime.id ? " active" : ""}`}
                key={anime.id}
                onClick={() => chooseAnime(anime)}
                type="button"
              >
                <img src={anime.coverImage?.large || anime.coverImage?.extraLarge || ""} alt="" loading="lazy" />
                <span className="ar-result-copy">
                  <strong>{displayTitle(anime)}</strong>
                  <small>{[anime.format, anime.seasonYear, anime.episodes ? `${anime.episodes} episodes` : "Episode count unavailable"].filter(Boolean).join(" · ")}</small>
                </span>
                {selected?.id === anime.id && <Check size={18} aria-label="Selected" />}
              </button>
            ))}
          </div>
        </div>

        <div className="ar-selection">
          {!selected ? (
            <div className="ar-placeholder">
              <Search size={27} />
              <h2>Choose a series</h2>
              <p>AniList matches will appear here as you type.</p>
            </div>
          ) : (
            <>
              <div className="ar-selected-title">
                <img src={selected.coverImage?.extraLarge || selected.coverImage?.large || ""} alt="" />
                <div>
                  <span className="ar-kicker">SELECTED SERIES</span>
                  <h2>{displayTitle(selected)}</h2>
                  <p>{[selected.format, selected.seasonYear, selected.episodes ? `${selected.episodes} episodes` : "Episode count unavailable"].filter(Boolean).join(" · ")}</p>
                </div>
              </div>
              {selected.description && <p className="ar-description">{selected.description.replace(/<[^>]*>/g, " ").replace(/\s+/g, " ").trim()}</p>}
              <h3>Choose an episode</h3>
              {selected.episodes && selected.episodes > 0 ? (
                <div className="ar-episodes">
                  {Array.from({ length: selected.episodes }, (_, index) => index + 1).map((episode) => (
                    <button key={episode} disabled={busy} onClick={() => void startDownload(episode)} type="button" title={`Download episode ${episode}`}>
                      {busy ? <LoaderCircle className="ar-spinner" size={15} /> : <Download size={15} />}
                      Episode {episode}
                    </button>
                  ))}
                </div>
              ) : (
                <p className="ar-feedback">AniList does not list an episode count for this title, so episode selection is unavailable.</p>
              )}
              {message && <p className="ar-feedback success" role="status"><Check size={16} />{message}</p>}
              {error && <p className="ar-feedback error" role="alert">{error}</p>}
            </>
          )}
        </div>
      </div>
    </section>
  );
}
