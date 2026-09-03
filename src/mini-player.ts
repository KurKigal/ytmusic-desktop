import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import {
  isLanguage,
  translations,
  type Language,
  type TranslationDictionary,
} from "./i18n";

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

type LocalizedText = (dictionary: TranslationDictionary) => string;

const miniPlayer = requireElement<HTMLElement>("#mini-player");
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
const playbackControls = requireElement<HTMLDivElement>("#playback-controls");

let playerState: MiniPlayerState | null = null;
let stateRevision = 0;
let languageRevision = 0;
let isSeeking = false;
let seekPending = false;
let requestedArtworkUrl: string | null = null;
let loadedArtworkUrl: string | null = null;
let failedArtworkUrl: string | null = null;
let noticeTimer: number | undefined;
let noticeText: LocalizedText | null = null;
let currentLanguage: Language = "en";
let unlistenPlayerState: UnlistenFn | undefined;
let unlistenLanguage: UnlistenFn | undefined;

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`Required mini-player element is missing: ${selector}`);
  return element;
}

function dictionary(): TranslationDictionary {
  return translations[currentLanguage];
}

function applyLanguage(language: Language): void {
  currentLanguage = language;
  const copy = dictionary().miniPlayer;

  document.documentElement.lang = language;
  document.title = copy.windowTitle;
  miniPlayer.setAttribute("aria-label", copy.ariaLabel);
  playbackControls.setAttribute("aria-label", copy.playbackControls);
  previousButton.ariaLabel = copy.previousTrack;
  previousButton.title = copy.previousTrack;
  nextButton.ariaLabel = copy.nextTrack;
  nextButton.title = copy.nextTrack;
  progress.ariaLabel = copy.trackPosition;

  renderState();
  renderNotice();
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
  const copy = dictionary().miniPlayer;

  title.textContent = playerState?.title?.trim() || copy.waitingTitle;
  artist.textContent = playerState?.artist?.trim() || copy.waitingArtist;

  previousButton.disabled = !hasPlayback;
  playPauseButton.disabled = !hasPlayback;
  nextButton.disabled = !hasPlayback;
  playPauseButton.ariaLabel = isPlaying ? copy.pause : copy.play;
  playPauseButton.title = isPlaying ? copy.pause : copy.play;
  playIcon.toggleAttribute("hidden", isPlaying);
  pauseIcon.toggleAttribute("hidden", !isPlaying);

  renderArtwork(playerState?.artworkUrl ?? null, playerState?.title ?? null);
  if (!isSeeking) renderTimeline();
}

function renderArtwork(url: string | null, trackTitle: string | null): void {
  const copy = dictionary().miniPlayer;
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

  artwork.alt = trackTitle?.trim() ? copy.artworkFor(trackTitle.trim()) : copy.trackArtwork;

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
  progress.setAttribute(
    "aria-valuetext",
    dictionary().miniPlayer.timelineValue(elapsedText, durationText),
  );

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
    const detail = formatError(error);
    showNotice((copy) => copy.miniPlayer.errors.playbackControl(detail));
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
    const detail = formatError(error);
    showNotice((copy) => copy.miniPlayer.errors.seek(detail));
  } finally {
    seekPending = false;
    isSeeking = false;
    renderTimeline();
  }
}

function showNotice(message: LocalizedText): void {
  if (noticeTimer !== undefined) window.clearTimeout(noticeTimer);
  noticeText = message;
  renderNotice();
  notice.classList.add("is-visible");
  noticeTimer = window.setTimeout(clearNotice, 5000);
}

function renderNotice(): void {
  notice.textContent = noticeText?.(dictionary()) ?? "";
}

function clearNotice(): void {
  if (noticeTimer !== undefined) window.clearTimeout(noticeTimer);
  noticeTimer = undefined;
  noticeText = null;
  renderNotice();
  notice.classList.remove("is-visible");
}

function formatError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "message" in error) {
    return String(error.message);
  }
  return dictionary().common.unexpectedError;
}

async function initializePlayerState(): Promise<void> {
  try {
    unlistenPlayerState = await listen<MiniPlayerState | null>("mini-player-state", ({ payload }) => {
      stateRevision += 1;
      applyState(payload);
    });

    const revisionBeforeFetch = stateRevision;
    const initialState = await invoke<MiniPlayerState | null>("get_mini_player_state");
    if (stateRevision === revisionBeforeFetch) applyState(initialState);
  } catch (error: unknown) {
    const detail = formatError(error);
    showNotice((copy) => copy.miniPlayer.errors.playbackConnection(detail));
  }
}

async function initializeLanguage(): Promise<void> {
  try {
    unlistenLanguage = await listen<Language>("local-ui-language-changed", ({ payload }) => {
      if (!isLanguage(payload)) return;
      languageRevision += 1;
      applyLanguage(payload);
    });

    const revisionBeforeFetch = languageRevision;
    const language = await invoke<Language>("get_local_ui_language");
    if (languageRevision === revisionBeforeFetch && isLanguage(language)) applyLanguage(language);
  } catch (error: unknown) {
    const detail = formatError(error);
    showNotice((copy) => copy.miniPlayer.errors.languageConnection(detail));
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
  unlistenPlayerState?.();
  unlistenLanguage?.();
});

applyLanguage(currentLanguage);
void initializePlayerState();
void initializeLanguage();
