import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

type Playback = "playing" | "paused" | "inactive";

interface MiniPlayerState {
  title: string | null;
  artist: string | null;
  artworkUrl: string | null;
  playback: Playback;
  position: number;
  duration: number;
}

type MiniPlayerCommand =
  | { type: "togglePlayback" }
  | { type: "previous" }
  | { type: "next" }
  | { type: "seek"; position: number };

const artwork = requireElement<HTMLImageElement>("#artwork");
const artworkPlaceholder = requireElement<HTMLDivElement>("#artwork-placeholder");
const title = requireElement<HTMLHeadingElement>("#track-title");
const artist = requireElement<HTMLParagraphElement>("#track-artist");
const previousButton = requireElement<HTMLButtonElement>("#previous");
const playPauseButton = requireElement<HTMLButtonElement>("#play-pause");
const nextButton = requireElement<HTMLButtonElement>("#next");
const playIcon = requireElement<SVGElement>("#play-icon");
const pauseIcon = requireElement<SVGElement>("#pause-icon");
const progress = requireElement<HTMLInputElement>("#progress");
const elapsed = requireElement<HTMLSpanElement>("#elapsed");
const duration = requireElement<HTMLSpanElement>("#duration");
const notice = requireElement<HTMLParagraphElement>("#notice");

let playerState: MiniPlayerState | null = null;
let stateRevision = 0;
let isSeeking = false;
let seekPending = false;
let requestedArtworkUrl: string | null = null;
let loadedArtworkUrl: string | null = null;
let failedArtworkUrl: string | null = null;
let noticeTimer: number | undefined;
let unlisten: UnlistenFn | undefined;

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`Required mini-player element is missing: ${selector}`);
  return element;
}

function normalizeState(state: MiniPlayerState | null): MiniPlayerState | null {
  if (!state) return null;

  const safeDuration = finiteNonNegative(state.duration);
  return {
    ...state,
    position: Math.min(finiteNonNegative(state.position), safeDuration || Number.MAX_SAFE_INTEGER),
    duration: safeDuration,
  };
}

function finiteNonNegative(value: number): number {
  return Number.isFinite(value) ? Math.max(0, value) : 0;
}

function applyState(state: MiniPlayerState | null): void {
  playerState = normalizeState(state);
  renderState();
}

function renderState(): void {
  const hasPlayback = playerState !== null && playerState.playback !== "inactive";
  const isPlaying = playerState?.playback === "playing";

  title.textContent = playerState?.title?.trim() || "Waiting for playback";
  artist.textContent = playerState?.artist?.trim() || "Start playing music in YouTube Music";

  previousButton.disabled = !hasPlayback;
  playPauseButton.disabled = !hasPlayback;
  nextButton.disabled = !hasPlayback;
  playPauseButton.ariaLabel = isPlaying ? "Pause" : "Play";
  playPauseButton.title = isPlaying ? "Pause" : "Play";
  playIcon.toggleAttribute("hidden", isPlaying);
  pauseIcon.toggleAttribute("hidden", !isPlaying);

  renderArtwork(playerState?.artworkUrl ?? null, playerState?.title ?? null);
  if (!isSeeking) renderTimeline();
}

function renderArtwork(url: string | null, trackTitle: string | null): void {
  const nextUrl = url?.trim() ?? "";
  if (!nextUrl) {
    artwork.removeAttribute("src");
    artwork.alt = "";
    requestedArtworkUrl = null;
    loadedArtworkUrl = null;
    failedArtworkUrl = null;
    showArtworkPlaceholder();
    return;
  }

  artwork.alt = trackTitle?.trim() ? `Artwork for ${trackTitle.trim()}` : "Track artwork";

  if (requestedArtworkUrl !== nextUrl) {
    requestedArtworkUrl = nextUrl;
    loadedArtworkUrl = null;
    failedArtworkUrl = null;
    showArtworkPlaceholder();
    artwork.src = nextUrl;
  }

  if (loadedArtworkUrl === nextUrl && failedArtworkUrl !== nextUrl) {
    artwork.hidden = false;
    artworkPlaceholder.hidden = true;
  } else {
    showArtworkPlaceholder();
  }
}

function showArtworkPlaceholder(): void {
  artwork.hidden = true;
  artworkPlaceholder.hidden = false;
}

function renderTimeline(): void {
  const total = finiteNonNegative(playerState?.duration ?? 0);
  const current = Math.min(finiteNonNegative(playerState?.position ?? 0), total || 0);

  progress.max = String(total);
  progress.value = String(current);
  progress.disabled = total <= 0 || playerState?.playback === "inactive";
  updateTimelineLabels(current, total);
}

function updateTimelineLabels(current: number, total: number): void {
  const elapsedText = formatTime(current);
  const durationText = formatTime(total);
  elapsed.textContent = elapsedText;
  duration.textContent = durationText;
  progress.setAttribute("aria-valuetext", `${elapsedText} of ${durationText}`);

  const percentage = total > 0 ? Math.min(100, Math.max(0, (current / total) * 100)) : 0;
  progress.style.setProperty("--progress", `${percentage}%`);
}

function formatTime(seconds: number): string {
  const wholeSeconds = Math.floor(finiteNonNegative(seconds));
  const hours = Math.floor(wholeSeconds / 3600);
  const minutes = Math.floor((wholeSeconds % 3600) / 60);
  const remainingSeconds = wholeSeconds % 60;

  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(remainingSeconds).padStart(2, "0")}`;
  }
  return `${minutes}:${String(remainingSeconds).padStart(2, "0")}`;
}

async function sendCommand(command: MiniPlayerCommand, source: HTMLButtonElement): Promise<void> {
  source.disabled = true;
  source.setAttribute("aria-busy", "true");
  clearNotice();
  try {
    await invoke("control_mini_player", { command });
  } catch (error: unknown) {
    showNotice(`Playback control failed: ${formatError(error)}`);
  } finally {
    source.removeAttribute("aria-busy");
    renderState();
  }
}

async function seekTo(position: number): Promise<void> {
  const target = Math.min(finiteNonNegative(position), finiteNonNegative(playerState?.duration ?? 0));
  seekPending = true;
  progress.disabled = true;
  clearNotice();

  try {
    await invoke("control_mini_player", { command: { type: "seek", position: target } });
    if (playerState) playerState = { ...playerState, position: target };
  } catch (error: unknown) {
    showNotice(`Could not seek: ${formatError(error)}`);
  } finally {
    seekPending = false;
    isSeeking = false;
    renderTimeline();
  }
}

function showNotice(message: string): void {
  if (noticeTimer !== undefined) window.clearTimeout(noticeTimer);
  notice.textContent = message;
  notice.classList.add("is-visible");
  noticeTimer = window.setTimeout(clearNotice, 5000);
}

function clearNotice(): void {
  if (noticeTimer !== undefined) window.clearTimeout(noticeTimer);
  noticeTimer = undefined;
  notice.textContent = "";
  notice.classList.remove("is-visible");
}

function formatError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "message" in error) {
    return String(error.message);
  }
  return "An unexpected error occurred.";
}

async function initialize(): Promise<void> {
  try {
    unlisten = await listen<MiniPlayerState | null>("mini-player-state", ({ payload }) => {
      stateRevision += 1;
      applyState(payload);
    });

    const revisionBeforeFetch = stateRevision;
    const initialState = await invoke<MiniPlayerState | null>("get_mini_player_state");
    if (stateRevision === revisionBeforeFetch) applyState(initialState);
  } catch (error: unknown) {
    showNotice(`Could not connect to playback state: ${formatError(error)}`);
  }
}

artwork.addEventListener("error", () => {
  failedArtworkUrl = requestedArtworkUrl;
  loadedArtworkUrl = null;
  showArtworkPlaceholder();
});
artwork.addEventListener("load", () => {
  loadedArtworkUrl = requestedArtworkUrl;
  failedArtworkUrl = null;
  artwork.hidden = false;
  artworkPlaceholder.hidden = true;
});

previousButton.addEventListener("click", () => {
  void sendCommand({ type: "previous" }, previousButton);
});
playPauseButton.addEventListener("click", () => {
  void sendCommand({ type: "togglePlayback" }, playPauseButton);
});
nextButton.addEventListener("click", () => {
  void sendCommand({ type: "next" }, nextButton);
});

progress.addEventListener("input", () => {
  isSeeking = true;
  updateTimelineLabels(Number(progress.value), finiteNonNegative(playerState?.duration ?? 0));
});
progress.addEventListener("change", () => {
  void seekTo(Number(progress.value));
});
progress.addEventListener("pointercancel", () => {
  if (!seekPending) {
    isSeeking = false;
    renderTimeline();
  }
});

window.addEventListener("beforeunload", () => {
  unlisten?.();
});

renderState();
void initialize();
