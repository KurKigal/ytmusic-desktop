(() => {
  "use strict";

  const YOUTUBE_MUSIC_ORIGIN = "https://music.youtube.com";
  const SYNC_INTERVAL_MS = 1000;

  // Only run on YouTube Music.
  if (window.location.origin !== YOUTUBE_MUSIC_ORIGIN) {
    return;
  }

  // Only run inside the main frame.
  if (window.top !== window) {
    return;
  }

  // Prevent duplicate initialization.
  if (window.__YTMDESKTOP__) {
    return;
  }

  /**
   * Finds the active HTML media element used by YouTube Music.
   *
   * YouTube Music currently uses a <video> element for playback,
   * but we intentionally support both <audio> and <video>.
   */
  const getMediaElement = () => {
    const elements = [
      ...document.querySelectorAll("audio, video"),
    ];

    // Prefer the currently playing media element.
    const playing = elements.find(
      (element) =>
        !element.paused &&
        Number.isFinite(element.duration) &&
        element.readyState > 0
    );

    if (playing) {
      return playing;
    }

    // Fall back to any initialized media element.
    return (
      elements.find(
        (element) =>
          Number.isFinite(element.duration) &&
          element.readyState > 0
      ) ?? null
    );
  };

  /**
   * Reads track metadata from the browser Media Session API.
   */
  const getMetadata = () => {
    const metadata = navigator.mediaSession?.metadata;

    if (!metadata) {
      return null;
    }

    return {
      title: metadata.title || null,
      artist: metadata.artist || null,
      album: metadata.album || null,

      artwork: Array.from(metadata.artwork ?? []).map(
        (item) => ({
          src: item.src,
          sizes: item.sizes || null,
          type: item.type || null,
        })
      ),
    };
  };

  /**
   * Creates a serializable snapshot of the current player state.
   *
   * This object is sent directly to the Rust core through Tauri IPC.
   */
  const getState = () => {
    const media = getMediaElement();

    return {
      metadata: getMetadata(),

      playback:
        navigator.mediaSession?.playbackState ?? "none",

      position:
        media && Number.isFinite(media.currentTime)
          ? media.currentTime
          : 0,

      duration:
        media && Number.isFinite(media.duration)
          ? media.duration
          : 0,

      paused: media?.paused ?? true,

      mediaType: media?.tagName ?? null,
    };
  };

  /**
   * Starts playback.
   */
  const play = async () => {
    const media = getMediaElement();

    if (!media) {
      throw new Error(
        "No active YouTube Music media element found."
      );
    }

    await media.play();
  };

  /**
   * Pauses playback.
   */
  const pause = () => {
    const media = getMediaElement();

    if (!media) {
      throw new Error(
        "No active YouTube Music media element found."
      );
    }

    media.pause();
  };

  /**
   * Toggles between playing and paused states.
   */
  const togglePlayback = async () => {
    const media = getMediaElement();

    if (!media) {
      throw new Error(
        "No active YouTube Music media element found."
      );
    }

    if (media.paused) {
      await media.play();
    } else {
      media.pause();
    }
  };

  /**
   * Seeks to an absolute position in seconds.
   */
  const seek = (seconds) => {
    const media = getMediaElement();

    if (!media) {
      throw new Error(
        "No active YouTube Music media element found."
      );
    }

    if (!Number.isFinite(seconds)) {
      throw new TypeError(
        "seek(seconds) expects a finite number."
      );
    }

    const duration = Number.isFinite(media.duration)
      ? media.duration
      : seconds;

    const target = Math.max(
      0,
      Math.min(seconds, duration)
    );

    media.currentTime = target;
  };

  /**
   * Finds the first matching clickable player button.
   */
  const findButton = (...selectors) => {
    for (const selector of selectors) {
      const element = document.querySelector(selector);

      if (element instanceof HTMLElement) {
        return element;
      }
    }

    return null;
  };

  /**
   * Skips to the next track.
   */
  const next = () => {
    const button = findButton(
      ".next-button",
      "tp-yt-paper-icon-button.next-button"
    );

    if (!button) {
      throw new Error(
        "YouTube Music next button was not found."
      );
    }

    button.click();
  };

  /**
   * Goes to the previous track.
   */
  const previous = () => {
    const button = findButton(
      ".previous-button",
      "tp-yt-paper-icon-button.previous-button"
    );

    if (!button) {
      throw new Error(
        "YouTube Music previous button was not found."
      );
    }

    button.click();
  };


  /**
 * Executes a playback command requested by the Rust core.
 */
const executeNativeCommand = async (command) => {
  if (
    !command ||
    typeof command !== "object" ||
    typeof command.type !== "string"
  ) {
    throw new TypeError(
      "Invalid native player command."
    );
  }

  switch (command.type) {
    case "play":
      await play();
      break;

    case "pause":
      pause();
      break;

    case "togglePlayback":
      await togglePlayback();
      break;

    case "next":
      next();
      break;

    case "previous":
      previous();
      break;

    case "seek":
      seek(command.position);
      break;

    default:
      throw new Error(
        `Unknown native player command: ${command.type}`
      );
  }
};

  // ---------------------------------------------------------
  // Native state synchronization
  // ---------------------------------------------------------

  let syncTimer = null;
  let syncInFlight = false;

  /**
   * Sends the current player state to the Rust core.
   */
  const publishState = async () => {
    // Avoid overlapping IPC requests.
    if (syncInFlight) {
      return;
    }

    const invoke = window.__TAURI__?.core?.invoke;

    if (typeof invoke !== "function") {
      console.warn(
        "[YTMusic Desktop] Tauri IPC is not available"
      );

      return;
    }

    syncInFlight = true;

    try {
      await invoke("update_player_state", {
        payload: getState(),
      });
    } catch (error) {
      console.error(
        "[YTMusic Desktop] failed to publish player state:",
        error
      );
    } finally {
      syncInFlight = false;
    }
  };

  /**
   * Starts periodic synchronization with the Rust core.
   */
  const startSync = () => {
    if (syncTimer !== null) {
      return;
    }

    // Immediately publish the initial state.
    void publishState();

    syncTimer = window.setInterval(() => {
      void publishState();
    }, SYNC_INTERVAL_MS);

    console.info(
      "[YTMusic Desktop] native player state sync started"
    );
  };

  /**
   * Stops periodic state synchronization.
   */
  const stopSync = () => {
    if (syncTimer === null) {
      return;
    }

    window.clearInterval(syncTimer);

    syncTimer = null;

    console.info(
      "[YTMusic Desktop] native player state sync stopped"
    );
  };

  // ---------------------------------------------------------
  // Public adapter API
  // ---------------------------------------------------------

  window.__YTMDESKTOP__ = Object.freeze({
    version: 1,

    // State
    getState,
    publishState,
    startSync,
    stopSync,

    // Playback controls
    play,
    pause,
    togglePlayback,
    seek,
    next,
    previous,
    
    // Native command bridge
    executeNativeCommand,
  });

  // ---------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------

  if (document.readyState === "loading") {
    document.addEventListener(
      "DOMContentLoaded",
      startSync,
      {
        once: true,
      }
    );
  } else {
    startSync();
  }

  window.addEventListener(
    "beforeunload",
    stopSync,
    {
      once: true,
    }
  );

  console.info(
    "[YTMusic Desktop] adapter initialized"
  );
})();