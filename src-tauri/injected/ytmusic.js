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

  const mediaRecords = new WeakMap();

  // Resource-generation IDs let Rust recognize asynchronous handoffs. A new
  // ID is exposed only after loadedmetadata makes its timing usable.
  const createOpaqueIdPrefix = () => {
    if (typeof window.crypto?.getRandomValues === "function") {
      try {
        const values = new Uint32Array(2);

        window.crypto.getRandomValues(values);

        return Array.from(values, (value) =>
          String(value).padStart(10, "0")
        ).join("");
      } catch {
        // Fall back to a timestamp if secure randomness is unavailable.
      }
    }

    return String(Date.now());
  };

  const opaqueIdPrefix = createOpaqueIdPrefix();

  let nextOpaqueId = 1;
  let mediaEventSequence = 0;
  let selectedMediaElement = null;
  let selectedMetadataFingerprint = null;
  let selectedMetadataId = null;
  let mediaSessionTiming = null;
  let lastMediaSessionTiming = null;
  let hasObservedMediaSessionTiming = false;
  let startupDomTiming = null;
  let pendingStartupDomObservation = null;
  let startupDomTimelineMetadataId = null;
  let startupDomTimelineId = null;
  let explicitSeekPending = false;

  const allocateOpaqueId = () => {
    const opaqueId =
      opaqueIdPrefix + String(nextOpaqueId).padStart(16, "0");

    nextOpaqueId =
      nextOpaqueId >= Number.MAX_SAFE_INTEGER
        ? 1
        : nextOpaqueId + 1;

    return opaqueId;
  };

  const nextMediaEventSequence = () => {
    mediaEventSequence += 1;

    return mediaEventSequence;
  };

  const getMediaRecord = (element) => {
    let record = mediaRecords.get(element);

    if (!record) {
      record = {
        mediaId: allocateOpaqueId(),
        pendingMediaId: null,
        lastActive: 0,
        lastInitialized: nextMediaEventSequence(),
        resourceLoading: false,
        hasLoadedMetadata: false,
      };

      mediaRecords.set(element, record);
    }

    return record;
  };

  const refreshMediaRecord = (element) => {
    const record = getMediaRecord(element);

    if (
      element.readyState >= HTMLMediaElement.HAVE_METADATA &&
      !record.hasLoadedMetadata
    ) {
      record.hasLoadedMetadata = true;
      record.lastInitialized = nextMediaEventSequence();
    }

    if (
      !element.paused &&
      !element.ended &&
      element.readyState >= HTMLMediaElement.HAVE_METADATA &&
      Number.isFinite(element.duration) &&
      element.duration > 0 &&
      record.lastActive === 0
    ) {
      record.lastActive = nextMediaEventSequence();
    }

    return record;
  };

  const isUsableMediaElement = (element) =>
    element.isConnected &&
    !getMediaRecord(element).resourceLoading &&
    !element.ended &&
    element.readyState >= HTMLMediaElement.HAVE_METADATA &&
    Number.isFinite(element.duration) &&
    element.duration > 0;

  const isPlayingMediaElement = (element) =>
    isUsableMediaElement(element) && !element.paused;

  const handleMediaEvent = (event) => {
    const element = event.target;

    if (!(element instanceof HTMLMediaElement)) {
      return;
    }

    const record = getMediaRecord(element);

    switch (event.type) {
      case "emptied":
      case "loadstart":
        if (!record.resourceLoading) {
          record.pendingMediaId = allocateOpaqueId();
        }

        record.resourceLoading = true;
        record.hasLoadedMetadata = false;
        record.lastInitialized = nextMediaEventSequence();

        if (selectedMediaElement === element) {
          materializeTimingPlaybackState(false);
          selectedMediaElement = null;
        }

        break;

      case "loadedmetadata":
        if (record.resourceLoading && record.pendingMediaId) {
          record.mediaId = record.pendingMediaId;
        }

        record.pendingMediaId = null;
        record.resourceLoading = false;
        record.hasLoadedMetadata = true;
        record.lastInitialized = nextMediaEventSequence();

        break;

      case "play":
      case "playing":
        refreshMediaRecord(element);
        record.lastActive = nextMediaEventSequence();

        if (isPlayingMediaElement(element)) {
          selectedMediaElement = element;
          materializeTimingPlaybackRate(
            element.playbackRate
          );
          materializeTimingPlaybackState(true);
        }

        break;

      case "pause":
        if (selectedMediaElement === element) {
          materializeTimingPlaybackState(false);
        }

        break;

      case "ratechange":
        if (selectedMediaElement === element) {
          materializeTimingPlaybackRate(
            element.playbackRate
          );
        }

        break;

      case "seeking":
        if (selectedMediaElement === element) {
          explicitSeekPending = true;
        }

        break;

      case "ended":
        if (selectedMediaElement === element) {
          materializeTimingPlaybackState(false);
          selectedMediaElement = null;
        }

        break;

      default:
        break;
    }

  };

  for (const eventName of [
    "emptied",
    "loadstart",
    "loadedmetadata",
    "play",
    "playing",
    "pause",
    "ratechange",
    "seeking",
    "ended",
  ]) {
    document.addEventListener(
      eventName,
      handleMediaEvent,
      true
    );
  }

  /**
   * Finds the active HTML media element used by YouTube Music.
   *
   * Selection is sticky while paused, but a newer playing element
   * takes precedence during a resource or element handoff.
   */
  const getMediaSelection = () => {
    const candidates = [
      ...document.querySelectorAll("audio, video"),
    ].map((element) => ({
      element,
      record: refreshMediaRecord(element),
    }));

    const sticky = candidates.find(
      ({ element }) =>
        element === selectedMediaElement &&
        isUsableMediaElement(element)
    );

    const playing = candidates.filter(({ element }) =>
      isPlayingMediaElement(element)
    );

    if (playing.length > 0) {
      const selected = playing.reduce((best, candidate) => {
        if (candidate.record.lastActive !== best.record.lastActive) {
          return candidate.record.lastActive > best.record.lastActive
            ? candidate
            : best;
        }

        if (candidate.element === selectedMediaElement) {
          return candidate;
        }

        if (best.element === selectedMediaElement) {
          return best;
        }

        return candidate.record.lastInitialized >=
          best.record.lastInitialized
          ? candidate
          : best;
      });

      selectedMediaElement = selected.element;

      return selected;
    }

    if (sticky) {
      return sticky;
    }

    const initialized = candidates.filter(({ element }) =>
      isUsableMediaElement(element)
    );

    if (initialized.length === 0) {
      selectedMediaElement = null;

      return null;
    }

    const selected = initialized.reduce((best, candidate) => {
      if (candidate.record.lastActive !== best.record.lastActive) {
        return candidate.record.lastActive > best.record.lastActive
          ? candidate
          : best;
      }

      return candidate.record.lastInitialized >=
        best.record.lastInitialized
        ? candidate
        : best;
    });

    selectedMediaElement = selected.element;

    return selected;
  };

  const requireMediaElement = () => {
    const selection = getMediaSelection();

    if (!selection) {
      throw new Error(
        "No active YouTube Music media element found."
      );
    }

    return selection.element;
  };

  /**
   * Reads track metadata from the browser Media Session API.
   */
  const getMetadataSelection = () => {
    const metadata = navigator.mediaSession?.metadata;

    if (!metadata) {
      return {
        metadata: null,
        metadataId: null,
      };
    }

    const value = {
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

    const fingerprint = JSON.stringify(value);

    // MediaMetadata wrapper identity is not a track-generation signal.
    if (fingerprint !== selectedMetadataFingerprint) {
      selectedMetadataFingerprint = fingerprint;
      selectedMetadataId = allocateOpaqueId();
    }

    return {
      metadata: value,
      metadataId: selectedMetadataId,
    };
  };

  const timingPositionAt = (
    timing,
    observedAt = performance.now()
  ) => {
    const elapsedSeconds = timing.advancing
      ? Math.max(
          0,
          (observedAt - timing.sampledAtMonotonicMs) / 1000
        ) * timing.playbackRate
      : 0;

    return Math.min(
      timing.duration,
      Math.max(0, timing.position + elapsedSeconds)
    );
  };

  const timingPlaybackIsAdvancing = () => {
    const playbackState =
      navigator.mediaSession?.playbackState;

    if (playbackState === "playing") {
      return true;
    }

    if (playbackState === "paused") {
      return false;
    }

    return Boolean(
      selectedMediaElement &&
      !selectedMediaElement.paused &&
      !selectedMediaElement.ended
    );
  };

  const materializeTimingPlaybackState = (
    advancing,
    observedAt = performance.now()
  ) => {
    const materialize = (timing) => {
      if (!timing || timing.advancing === advancing) {
        return timing;
      }

      return {
        ...timing,
        position: timingPositionAt(timing, observedAt),
        sampledAtMonotonicMs: observedAt,
        advancing,
      };
    };

    if (mediaSessionTiming) {
      mediaSessionTiming = materialize(mediaSessionTiming);
      lastMediaSessionTiming = mediaSessionTiming;
    } else if (lastMediaSessionTiming) {
      lastMediaSessionTiming = materialize(
        lastMediaSessionTiming
      );
    }

    if (startupDomTiming) {
      startupDomTiming = materialize(startupDomTiming);
    }
  };

  const materializeTimingPlaybackRate = (
    playbackRate,
    observedAt = performance.now()
  ) => {
    if (
      !Number.isFinite(playbackRate) ||
      playbackRate <= 0
    ) {
      return;
    }

    const materialize = (timing) => {
      if (!timing || timing.playbackRate === playbackRate) {
        return timing;
      }

      return {
        ...timing,
        position: timingPositionAt(timing, observedAt),
        playbackRate,
        sampledAtMonotonicMs: observedAt,
      };
    };

    if (mediaSessionTiming) {
      mediaSessionTiming = materialize(mediaSessionTiming);
      lastMediaSessionTiming = mediaSessionTiming;
    } else if (lastMediaSessionTiming) {
      lastMediaSessionTiming = materialize(
        lastMediaSessionTiming
      );
    }

    if (startupDomTiming) {
      startupDomTiming = materialize(startupDomTiming);
    }
  };

  const isSameMetadataTimelineReset = (
    previous,
    position,
    observedAt,
    followsExplicitSeek
  ) => {
    if (!previous) {
      return false;
    }

    const previousPosition = timingPositionAt(
      previous,
      observedAt
    );

    return (
      position === 0 &&
      previousPosition > 0 &&
      !followsExplicitSeek
    );
  };

  const recordMediaSessionPositionState = (state) => {
    if (state === undefined) {
      mediaSessionTiming = null;

      return;
    }

    const position = Number(state.position ?? 0);
    const duration = Number(state.duration);
    const playbackRate = Number(state.playbackRate ?? 1);

    if (
      !Number.isFinite(position) ||
      !Number.isFinite(duration) ||
      !Number.isFinite(playbackRate) ||
      position < 0 ||
      duration <= 0 ||
      playbackRate <= 0
    ) {
      mediaSessionTiming = null;

      return;
    }

    const observedAt = performance.now();
    const followsExplicitSeek =
      explicitSeekPending ||
      Boolean(selectedMediaElement?.seeking);

    // Consume the seek signal with the next valid timing observation.
    explicitSeekPending = false;

    const timingMetadataId =
      getMetadataSelection().metadataId;

    // A PositionState captured before MediaSession metadata cannot prove a
    // track pairing. Keep the guarded startup source available until the
    // first observation that can.
    if (!timingMetadataId) {
      mediaSessionTiming = null;

      return;
    }

    hasObservedMediaSessionTiming = true;
    startupDomTiming = null;
    pendingStartupDomObservation = null;
    startupDomTimelineMetadataId = null;
    startupDomTimelineId = null;

    const timingObservationId = allocateOpaqueId();
    const previous =
      mediaSessionTiming ?? lastMediaSessionTiming;
    const previousHasSameMetadata =
      previous?.timingMetadataId === timingMetadataId;
    const timelineReset =
      timingMetadataId !== null &&
      previousHasSameMetadata &&
      isSameMetadataTimelineReset(
        previous,
        position,
        observedAt,
        followsExplicitSeek
      );

    let timelineId = previous?.timelineId ?? null;

    if (
      timingMetadataId === null ||
      !previousHasSameMetadata
    ) {
      timelineId = timingMetadataId
        ? allocateOpaqueId()
        : null;
    } else if (timelineReset) {
      timelineId = allocateOpaqueId();
    }

    mediaSessionTiming = {
      position: Math.min(position, duration),
      duration,
      playbackRate,
      sampledAtMonotonicMs: observedAt,
      advancing: timingPlaybackIsAdvancing(),
      timingMetadataId,
      timelineId,
      timingObservationId,
    };

    lastMediaSessionTiming = mediaSessionTiming;
  };

  const installMediaSessionPositionObserver = () => {
    const mediaSession = navigator.mediaSession;
    const original = mediaSession?.setPositionState;

    if (!mediaSession || typeof original !== "function") {
      return;
    }

    const wrapped = function (...args) {
      const result = Reflect.apply(original, this, args);

      try {
        recordMediaSessionPositionState(args[0]);
      } catch {
        // Observation must not affect successful page behavior.
      }

      return result;
    };

    try {
      let owner = mediaSession;

      while (
        owner &&
        !Object.prototype.hasOwnProperty.call(
          owner,
          "setPositionState"
        )
      ) {
        owner = Object.getPrototypeOf(owner);
      }

      const descriptor = owner
        ? Object.getOwnPropertyDescriptor(
            owner,
            "setPositionState"
          )
        : null;

      if (owner && descriptor?.value === original) {
        Object.defineProperty(owner, "setPositionState", {
          ...descriptor,
          value: wrapped,
        });
      } else {
        Object.defineProperty(
          mediaSession,
          "setPositionState",
          {
            configurable: true,
            writable: true,
            value: wrapped,
          }
        );
      }
    } catch {
      // The guarded startup DOM source remains available if this fails.
    }
  };

  const resetStartupDomObservation = () => {
    startupDomTiming = null;
    pendingStartupDomObservation = null;
  };

  const observeStartupDomTiming = (
    metadataId,
    media,
    advancing,
    observedAt
  ) => {
    if (hasObservedMediaSessionTiming) {
      return;
    }

    // YTM can publish PositionState before this adapter is installed. Its
    // progress bar is track-relative, unlike the reused MSE video timeline.
    const progress = document.querySelector(
      '#progress-bar[role="progressbar"]'
    );
    const positionAttribute = progress?.getAttribute(
      "aria-valuenow"
    );
    const durationAttribute = progress?.getAttribute(
      "aria-valuemax"
    );

    if (
      !metadataId ||
      !media ||
      !positionAttribute?.trim() ||
      !durationAttribute?.trim()
    ) {
      resetStartupDomObservation();

      return;
    }

    const position = Number(positionAttribute);
    const duration = Number(durationAttribute);

    if (
      !Number.isFinite(position) ||
      !Number.isFinite(duration) ||
      position < 0 ||
      duration <= 0 ||
      position > duration
    ) {
      resetStartupDomObservation();

      return;
    }

    const mediaPlaybackRate = Number(media.playbackRate);
    const playbackRate =
      Number.isFinite(mediaPlaybackRate) &&
      mediaPlaybackRate > 0
        ? mediaPlaybackRate
        : 1;
    const observation = {
      metadataId,
      position,
      duration,
      timingObservationId: allocateOpaqueId(),
    };
    const previous = pendingStartupDomObservation;
    const coherent = Boolean(
      previous &&
      previous.metadataId === metadataId &&
      previous.duration === duration &&
      position >= previous.position
    );

    if (startupDomTimelineMetadataId !== metadataId) {
      startupDomTimelineMetadataId = metadataId;
      startupDomTimelineId = allocateOpaqueId();
    }

    pendingStartupDomObservation = observation;

    if (!coherent) {
      startupDomTiming = null;

      return;
    }

    startupDomTiming = {
      position,
      duration,
      playbackRate,
      sampledAtMonotonicMs: observedAt,
      advancing,
      timingMetadataId: metadataId,
      timelineId: startupDomTimelineId,
      timingObservationId:
        observation.timingObservationId,
    };
  };

  const getCurrentTiming = () =>
    mediaSessionTiming ??
    (!hasObservedMediaSessionTiming
      ? startupDomTiming
      : null);

  const getPairedTimingSnapshot = (
    observedAt = performance.now()
  ) => {
    const timing = getCurrentTiming();

    if (
      !timing?.timingMetadataId ||
      !timing.timelineId ||
      !timing.timingObservationId
    ) {
      return null;
    }

    return {
      position: timingPositionAt(
        timing,
        observedAt
      ),
      duration: timing.duration,
      playbackRate: timing.playbackRate,
      timingMetadataId: timing.timingMetadataId,
      timelineId: timing.timelineId,
      timingObservationId:
        timing.timingObservationId,
    };
  };

  const getRawTimingSnapshot = (
    observedAt = performance.now()
  ) => {
    const timing = getCurrentTiming();

    if (!timing) {
      return null;
    }

    return {
      position: timingPositionAt(
        timing,
        observedAt
      ),
      duration: timing.duration,
      playbackRate: timing.playbackRate,
    };
  };

  installMediaSessionPositionObserver();

  /**
   * Creates a serializable snapshot of the current player state.
   *
   * This object is sent directly to the Rust core through Tauri IPC.
   */
  const getState = () => {
    const selection = getMediaSelection();
    const media = selection?.element ?? null;
    const metadataSelection = getMetadataSelection();

    const ended = media?.ended ?? false;
    const paused = !media || ended || media.paused;

    const playback = !media
      ? "none"
      : paused
        ? "paused"
        : "playing";

    const observedAt = performance.now();

    materializeTimingPlaybackState(
      playback === "playing",
      observedAt
    );

    observeStartupDomTiming(
      metadataSelection.metadataId,
      media,
      playback === "playing",
      observedAt
    );

    const timing = getPairedTimingSnapshot(observedAt);

    return {
      metadata: metadataSelection.metadata,

      metadataId: metadataSelection.metadataId,

      playback,

      position: timing?.position ?? 0,

      duration: timing?.duration ?? 0,

      playbackRate: timing?.playbackRate ?? 1,

      paused,

      mediaType: media?.tagName ?? null,

      mediaId: selection?.record.mediaId ?? null,

      timingMetadataId:
        timing?.timingMetadataId ?? null,

      timelineId: timing?.timelineId ?? null,

      timingObservationId:
        timing?.timingObservationId ?? null,
    };
  };

  /**
   * Starts playback.
   */
  const play = async () => {
    const media = requireMediaElement();

    await media.play();
  };

  /**
   * Pauses playback.
   */
  const pause = () => {
    const media = requireMediaElement();

    media.pause();
  };

  /**
   * Toggles between playing and paused states.
   */
  const togglePlayback = async () => {
    const media = requireMediaElement();

    if (media.paused) {
      await media.play();
    } else {
      media.pause();
    }
  };

  /**
   * Seeks to an absolute position in seconds.
   */
  const seekMedia = (media, seconds) => {
    if (!Number.isFinite(seconds)) {
      throw new TypeError(
        "seek(seconds) expects a finite number."
      );
    }

    const timing = getRawTimingSnapshot();

    if (timing && Number.isFinite(media.currentTime)) {
      const logicalTarget = Math.max(
        0,
        Math.min(seconds, timing.duration)
      );
      const elementTimelineOffset =
        media.currentTime - timing.position;
      const translatedTarget =
        elementTimelineOffset + logicalTarget;
      const target = Math.max(0, translatedTarget);

      explicitSeekPending = true;
      media.currentTime = target;

      return;
    }

    const fallbackDuration = Number.isFinite(media.duration)
      ? media.duration
      : seconds;
    const target = Math.max(
      0,
      Math.min(seconds, fallbackDuration)
    );

    explicitSeekPending = true;
    media.currentTime = target;
  };

  const seek = (seconds) => {
    const media = requireMediaElement();

    seekMedia(media, seconds);
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

    case "stop": {
      const media = requireMediaElement();

      media.pause();
      seekMedia(media, 0);
      break;
    }

    case "next":
      next();
      break;

    case "previous":
      previous();
      break;

    case "seek":
      seek(command.position);
      break;

    case "seekBy": {
      const media = requireMediaElement();
      const timing = getRawTimingSnapshot();
      const currentPosition = timing?.position ?? media.currentTime;

      seekMedia(
        media,
        currentPosition + command.offset
      );
      break;
    }

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
