(() => {
  "use strict";

  // Script yalnızca YouTube Music ana frame'inde çalışsın.
  if (window.location.origin !== "https://music.youtube.com") {
    return;
  }

  // Aynı sayfada script'in iki kere kurulmasını engelle.
  if (window.__YTMDESKTOP__) {
    return;
  }

  const getMediaElement = () => {
    const elements = [...document.querySelectorAll("audio, video")];

    // Öncelikle aktif olarak çalan elementi bul.
    const playing = elements.find(
      (element) =>
        !element.paused &&
        Number.isFinite(element.duration) &&
        element.readyState > 0
    );

    if (playing) {
      return playing;
    }

    // Çalmıyorsa hazır durumda olan elementi kullan.
    return (
      elements.find(
        (element) =>
          Number.isFinite(element.duration) &&
          element.readyState > 0
      ) ?? null
    );
  };

  const getMetadata = () => {
    const metadata = navigator.mediaSession?.metadata;

    if (!metadata) {
      return null;
    }

    return {
      title: metadata.title || null,
      artist: metadata.artist || null,
      album: metadata.album || null,

      artwork: Array.from(metadata.artwork ?? []).map((item) => ({
        src: item.src,
        sizes: item.sizes || null,
        type: item.type || null,
      })),
    };
  };

  const getState = () => {
    const media = getMediaElement();

    return {
      metadata: getMetadata(),

      playback: navigator.mediaSession?.playbackState ?? "none",

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

  const play = async () => {
    const media = getMediaElement();

    if (!media) {
      throw new Error("No active YouTube Music media element found.");
    }

    await media.play();
  };

  const pause = () => {
    const media = getMediaElement();

    if (!media) {
      throw new Error("No active YouTube Music media element found.");
    }

    media.pause();
  };

  const togglePlayback = async () => {
    const media = getMediaElement();

    if (!media) {
      throw new Error("No active YouTube Music media element found.");
    }

    if (media.paused) {
      await media.play();
    } else {
      media.pause();
    }
  };

  const seek = (seconds) => {
    const media = getMediaElement();

    if (!media) {
      throw new Error("No active YouTube Music media element found.");
    }

    if (!Number.isFinite(seconds)) {
      throw new TypeError("seek(seconds) expects a finite number.");
    }

    const target = Math.max(
      0,
      Math.min(seconds, media.duration || seconds)
    );

    media.currentTime = target;
  };

  const findButton = (...selectors) => {
    for (const selector of selectors) {
      const element = document.querySelector(selector);

      if (element instanceof HTMLElement) {
        return element;
      }
    }

    return null;
  };

  const next = () => {
    const button = findButton(
      ".next-button",
      "tp-yt-paper-icon-button.next-button"
    );

    if (!button) {
      throw new Error("YouTube Music next button was not found.");
    }

    button.click();
  };

  const previous = () => {
    const button = findButton(
      ".previous-button",
      "tp-yt-paper-icon-button.previous-button"
    );

    if (!button) {
      throw new Error("YouTube Music previous button was not found.");
    }

    button.click();
  };

  window.__YTMDESKTOP__ = Object.freeze({
    version: 1,

    getState,

    play,
    pause,
    togglePlayback,
    next,
    previous,
    seek,
  });

  console.info("[YTMusic Desktop] adapter initialized");
})();