export const LANGUAGES = ["en", "tr"] as const;

export type Language = (typeof LANGUAGES)[number];

export const SHORTCUT_ACTION_IDS = [
  "playPause",
  "next",
  "previous",
  "seekForward10",
  "seekBackward10",
] as const;

export type ShortcutAction = (typeof SHORTCUT_ACTION_IDS)[number];

interface ShortcutCopy {
  label: string;
  description: string;
}

export interface TranslationDictionary {
  common: {
    productName: string;
    unexpectedError: string;
  };
  settings: {
    windowTitle: string;
    heading: string;
    subtitle: string;
    application: {
      heading: string;
      description: string;
      language: ShortcutCopy;
      languageOptions: Record<Language, string>;
      discordRichPresenceEnabled: ShortcutCopy;
      closeToTray: ShortcutCopy;
      startMinimized: ShortcutCopy;
      miniPlayerAlwaysOnTop: ShortcutCopy;
    };
    shortcuts: {
      heading: string;
      description: string;
      restoreDefaults: string;
      pressShortcut: string;
      actions: Record<ShortcutAction, ShortcutCopy>;
      inputAriaLabel: (action: string) => string;
    };
    status: {
      loading: string;
      ready: string;
      loadFailed: (error: string) => string;
      pressCombination: (action: string) => string;
      changeCancelled: string;
      modifierOnly: string;
      unavailable: string;
      unchanged: string;
      duplicate: (shortcut: string, action: string) => string;
      uniqueRequired: string;
      applyingShortcut: (shortcut: string) => string;
      shortcutApplied: (shortcut: string) => string;
      shortcutFailed: (error: string) => string;
      savingApplication: string;
      applicationSaved: string;
      applicationFailed: (error: string) => string;
      restoringDefaults: string;
      defaultsRestored: string;
      restoreFailed: (error: string) => string;
    };
  };
  miniPlayer: {
    windowTitle: string;
    ariaLabel: string;
    waitingTitle: string;
    waitingArtist: string;
    playbackControls: string;
    previousTrack: string;
    nextTrack: string;
    play: string;
    pause: string;
    trackPosition: string;
    timelineValue: (elapsed: string, duration: string) => string;
    artworkFor: (title: string) => string;
    trackArtwork: string;
    errors: {
      playbackControl: (error: string) => string;
      seek: (error: string) => string;
      playbackConnection: (error: string) => string;
      languageConnection: (error: string) => string;
    };
  };
}

const english: TranslationDictionary = {
  common: {
    productName: "YTMusic Desktop",
    unexpectedError: "An unexpected error occurred.",
  },
  settings: {
    windowTitle: "YTMusic Desktop Settings",
    heading: "Settings",
    subtitle: "Customize how the app behaves on this computer.",
    application: {
      heading: "Application",
      description: "Choose how YTMusic Desktop behaves.",
      language: {
        label: "Language",
        description: "Language used by local app windows.",
      },
      languageOptions: {
        en: "English",
        tr: "Turkish",
      },
      discordRichPresenceEnabled: {
        label: "Discord Rich Presence",
        description: "Show the current track on your Discord profile.",
      },
      closeToTray: {
        label: "Close to tray",
        description: "Keep playback running by hiding the main window when it is closed.",
      },
      startMinimized: {
        label: "Start minimized",
        description: "Launch YTMusic Desktop hidden in the system tray.",
      },
      miniPlayerAlwaysOnTop: {
        label: "Mini Player always on top",
        description: "Keep the Mini Player above other windows.",
      },
    },
    shortcuts: {
      heading: "Global shortcuts",
      description: "Click a field, then press the shortcut you want to use.",
      restoreDefaults: "Restore All Defaults",
      pressShortcut: "Press shortcut",
      actions: {
        playPause: { label: "Play / Pause", description: "Toggle playback" },
        next: { label: "Next", description: "Play the next track" },
        previous: { label: "Previous", description: "Play the previous track" },
        seekForward10: {
          label: "Seek Forward 10s",
          description: "Skip ahead ten seconds",
        },
        seekBackward10: {
          label: "Seek Backward 10s",
          description: "Skip back ten seconds",
        },
      },
      inputAriaLabel: (action) => `${action} shortcut`,
    },
    status: {
      loading: "Loading settings…",
      ready: "Settings are up to date.",
      loadFailed: (error) => `Could not load settings: ${error}`,
      pressCombination: (action) => `Press a key combination for ${action}.`,
      changeCancelled: "Shortcut change cancelled.",
      modifierOnly: "Press a non-modifier key with any modifiers you want.",
      unavailable: "Settings are not available yet.",
      unchanged: "Shortcut is unchanged.",
      duplicate: (shortcut, action) => `${shortcut} is already assigned to ${action}.`,
      uniqueRequired: "Each action needs a unique shortcut.",
      applyingShortcut: (shortcut) => `Applying ${shortcut}…`,
      shortcutApplied: (shortcut) => `${shortcut} is ready to use.`,
      shortcutFailed: (error) => `Could not change the shortcut: ${error}`,
      savingApplication: "Saving application settings…",
      applicationSaved: "Application settings updated.",
      applicationFailed: (error) => `Could not update application settings: ${error}`,
      restoringDefaults: "Restoring defaults…",
      defaultsRestored: "Defaults restored.",
      restoreFailed: (error) => `Could not restore defaults: ${error}`,
    },
  },
  miniPlayer: {
    windowTitle: "YTMusic Desktop Mini Player",
    ariaLabel: "YTMusic Desktop mini player",
    waitingTitle: "Waiting for playback",
    waitingArtist: "Start playing music in YouTube Music",
    playbackControls: "Playback controls",
    previousTrack: "Previous track",
    nextTrack: "Next track",
    play: "Play",
    pause: "Pause",
    trackPosition: "Track position",
    timelineValue: (elapsed, duration) => `${elapsed} of ${duration}`,
    artworkFor: (title) => `Artwork for ${title}`,
    trackArtwork: "Track artwork",
    errors: {
      playbackControl: (error) => `Playback control failed: ${error}`,
      seek: (error) => `Could not seek: ${error}`,
      playbackConnection: (error) => `Could not connect to playback state: ${error}`,
      languageConnection: (error) => `Could not load the local UI language: ${error}`,
    },
  },
};

const turkish: TranslationDictionary = {
  common: {
    productName: "YTMusic Desktop",
    unexpectedError: "Beklenmeyen bir hata oluştu.",
  },
  settings: {
    windowTitle: "YTMusic Desktop Ayarları",
    heading: "Ayarlar",
    subtitle: "Uygulamanın bu bilgisayarda nasıl çalışacağını özelleştirin.",
    application: {
      heading: "Uygulama",
      description: "YTMusic Desktop'ın nasıl davranacağını seçin.",
      language: {
        label: "Dil",
        description: "Yerel uygulama pencerelerinde kullanılan dil.",
      },
      languageOptions: {
        en: "İngilizce",
        tr: "Türkçe",
      },
      discordRichPresenceEnabled: {
        label: "Discord Zengin Etkinliği",
        description: "Çalan parçayı Discord profilinizde gösterin.",
      },
      closeToTray: {
        label: "Sistem tepsisine kapat",
        description: "Ana pencere kapatıldığında gizleyerek oynatmayı sürdürün.",
      },
      startMinimized: {
        label: "Küçültülmüş başlat",
        description: "YTMusic Desktop'ı sistem tepsisinde gizli başlatın.",
      },
      miniPlayerAlwaysOnTop: {
        label: "Mini Oynatıcı her zaman üstte",
        description: "Mini Oynatıcıyı diğer pencerelerin üzerinde tutun.",
      },
    },
    shortcuts: {
      heading: "Genel kısayollar",
      description: "Bir alana tıklayın, ardından kullanmak istediğiniz kısayola basın.",
      restoreDefaults: "Tüm Varsayılanları Geri Yükle",
      pressShortcut: "Kısayola basın",
      actions: {
        playPause: { label: "Oynat / Duraklat", description: "Oynatmayı değiştir" },
        next: { label: "Sonraki", description: "Sonraki parçayı çal" },
        previous: { label: "Önceki", description: "Önceki parçayı çal" },
        seekForward10: {
          label: "10 sn İleri Sar",
          description: "On saniye ileri atla",
        },
        seekBackward10: {
          label: "10 sn Geri Sar",
          description: "On saniye geri atla",
        },
      },
      inputAriaLabel: (action) => `${action} kısayolu`,
    },
    status: {
      loading: "Ayarlar yükleniyor…",
      ready: "Ayarlar güncel.",
      loadFailed: (error) => `Ayarlar yüklenemedi: ${error}`,
      pressCombination: (action) => `${action} için bir tuş birleşimine basın.`,
      changeCancelled: "Kısayol değişikliği iptal edildi.",
      modifierOnly: "İstediğiniz değiştiricilerle birlikte değiştirici olmayan bir tuşa basın.",
      unavailable: "Ayarlar henüz kullanılamıyor.",
      unchanged: "Kısayol değişmedi.",
      duplicate: (shortcut, action) => `${shortcut} zaten ${action} eylemine atanmış.`,
      uniqueRequired: "Her eylem benzersiz bir kısayol gerektirir.",
      applyingShortcut: (shortcut) => `${shortcut} uygulanıyor…`,
      shortcutApplied: (shortcut) => `${shortcut} kullanıma hazır.`,
      shortcutFailed: (error) => `Kısayol değiştirilemedi: ${error}`,
      savingApplication: "Uygulama ayarları kaydediliyor…",
      applicationSaved: "Uygulama ayarları güncellendi.",
      applicationFailed: (error) => `Uygulama ayarları güncellenemedi: ${error}`,
      restoringDefaults: "Varsayılanlar geri yükleniyor…",
      defaultsRestored: "Varsayılanlar geri yüklendi.",
      restoreFailed: (error) => `Varsayılanlar geri yüklenemedi: ${error}`,
    },
  },
  miniPlayer: {
    windowTitle: "YTMusic Desktop Mini Oynatıcı",
    ariaLabel: "YTMusic Desktop mini oynatıcı",
    waitingTitle: "Oynatma bekleniyor",
    waitingArtist: "YouTube Music'te müzik çalmaya başlayın",
    playbackControls: "Oynatma denetimleri",
    previousTrack: "Önceki parça",
    nextTrack: "Sonraki parça",
    play: "Oynat",
    pause: "Duraklat",
    trackPosition: "Parça konumu",
    timelineValue: (elapsed, duration) => `${elapsed} / ${duration}`,
    artworkFor: (title) => `${title} kapak görseli`,
    trackArtwork: "Parça kapak görseli",
    errors: {
      playbackControl: (error) => `Oynatma denetimi başarısız oldu: ${error}`,
      seek: (error) => `İleri/geri sarılamadı: ${error}`,
      playbackConnection: (error) => `Oynatma durumuna bağlanılamadı: ${error}`,
      languageConnection: (error) => `Yerel arayüz dili yüklenemedi: ${error}`,
    },
  },
};

export const translations: Readonly<Record<Language, TranslationDictionary>> = {
  en: english,
  tr: turkish,
};

export function isLanguage(value: unknown): value is Language {
  return typeof value === "string" && LANGUAGES.includes(value as Language);
}
