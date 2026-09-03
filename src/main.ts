import { invoke } from "@tauri-apps/api/core";

import {
  LANGUAGES,
  SHORTCUT_ACTION_IDS,
  isLanguage,
  translations,
  type Language,
  type ShortcutAction,
  type TranslationDictionary,
} from "./i18n";

interface ShortcutSettings {
  playPause: string;
  next: string;
  previous: string;
  seekForward10: string;
  seekBackward10: string;
}

interface ApplicationSettings {
  language: Language;
  discordRichPresenceEnabled: boolean;
  closeToTray: boolean;
  startMinimized: boolean;
  miniPlayerAlwaysOnTop: boolean;
}

interface AppSettings {
  schemaVersion: number;
  application: ApplicationSettings;
  shortcuts: ShortcutSettings;
}

type BooleanApplicationSetting = Exclude<keyof ApplicationSettings, "language">;
type LocalizedText = (dictionary: TranslationDictionary) => string;
type StatusKind = "neutral" | "busy" | "success" | "error";

interface ShortcutElements {
  input: HTMLInputElement;
  label: HTMLLabelElement;
  description: HTMLParagraphElement;
  error: HTMLParagraphElement;
  errorText: LocalizedText | null;
}

interface SwitchElements {
  input: HTMLInputElement;
  label: HTMLLabelElement;
  description: HTMLParagraphElement;
}

const APPLICATION_SWITCHES = [
  "discordRichPresenceEnabled",
  "closeToTray",
  "startMinimized",
  "miniPlayerAlwaysOnTop",
] as const satisfies readonly BooleanApplicationSetting[];

const shortcutElements = new Map<ShortcutAction, ShortcutElements>();
const switchElements = new Map<BooleanApplicationSetting, SwitchElements>();
let settings: AppSettings | null = null;
let pendingAction: ShortcutAction | null = null;
let applicationUpdatePending = false;
let currentLanguage: Language = "en";
let currentStatusText: LocalizedText = (dictionary) => dictionary.settings.status.loading;
let currentStatusKind: StatusKind = "busy";

const productName = requireElement<HTMLParagraphElement>("#product-name");
const settingsHeading = requireElement<HTMLHeadingElement>("#settings-heading");
const settingsSubtitle = requireElement<HTMLParagraphElement>("#settings-subtitle");
const applicationHeading = requireElement<HTMLHeadingElement>("#application-heading");
const applicationDescription = requireElement<HTMLParagraphElement>("#application-description");
const applicationList = requireElement<HTMLDivElement>("#application-list");
const shortcutsHeading = requireElement<HTMLHeadingElement>("#shortcuts-heading");
const shortcutsDescription = requireElement<HTMLParagraphElement>("#shortcuts-description");
const shortcutList = requireElement<HTMLDivElement>("#shortcut-list");
const restoreButton = requireElement<HTMLButtonElement>("#restore-defaults");
const statusMessage = requireElement<HTMLParagraphElement>("#status-message");
const statusIndicator = requireElement<HTMLSpanElement>("#status-indicator");

let languageSelect: HTMLSelectElement;
let languageLabel: HTMLLabelElement;
let languageDescription: HTMLParagraphElement;

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`Required settings element is missing: ${selector}`);
  return element;
}

function dictionary(): TranslationDictionary {
  return translations[currentLanguage];
}

function renderApplicationFields(): void {
  const languageRow = document.createElement("div");
  languageRow.className = "setting-row";

  const languageCopy = document.createElement("div");
  languageCopy.className = "setting-copy";
  languageLabel = document.createElement("label");
  languageLabel.htmlFor = "application-language";
  languageDescription = document.createElement("p");
  languageDescription.id = "application-language-description";
  languageCopy.append(languageLabel, languageDescription);

  languageSelect = document.createElement("select");
  languageSelect.id = "application-language";
  languageSelect.className = "select-input";
  languageSelect.setAttribute("aria-describedby", languageDescription.id);
  for (const language of LANGUAGES) {
    const option = document.createElement("option");
    option.value = language;
    languageSelect.append(option);
  }
  languageSelect.addEventListener("change", () => {
    if (isLanguage(languageSelect.value)) {
      void updateApplicationSetting("language", languageSelect.value);
    }
  });

  languageRow.append(languageCopy, languageSelect);
  applicationList.append(languageRow);

  for (const setting of APPLICATION_SWITCHES) {
    const row = document.createElement("div");
    row.className = "setting-row";

    const copy = document.createElement("div");
    copy.className = "setting-copy";
    const label = document.createElement("label");
    label.htmlFor = `application-${setting}`;
    const description = document.createElement("p");
    description.id = `application-${setting}-description`;
    copy.append(label, description);

    const input = document.createElement("input");
    input.id = `application-${setting}`;
    input.className = "switch-input";
    input.type = "checkbox";
    input.setAttribute("role", "switch");
    input.setAttribute("aria-describedby", description.id);
    input.addEventListener("change", () => {
      void updateApplicationSetting(setting, input.checked);
    });

    row.append(copy, input);
    applicationList.append(row);
    switchElements.set(setting, { input, label, description });
  }
}

function renderShortcutFields(): void {
  for (const action of SHORTCUT_ACTION_IDS) {
    const row = document.createElement("div");
    row.className = "shortcut-row";

    const copy = document.createElement("div");
    copy.className = "shortcut-copy";
    const label = document.createElement("label");
    label.htmlFor = `shortcut-${action}`;
    const description = document.createElement("p");

    const control = document.createElement("div");
    control.className = "shortcut-control";
    const input = document.createElement("input");
    input.id = `shortcut-${action}`;
    input.className = "shortcut-input";
    input.type = "text";
    input.readOnly = true;
    input.autocomplete = "off";
    input.spellcheck = false;
    input.setAttribute("aria-describedby", `shortcut-error-${action}`);
    input.addEventListener("keydown", (event) => void captureShortcut(action, event));
    input.addEventListener("focus", () => {
      if (pendingAction === null && !applicationUpdatePending) {
        setStatus(
          (nextDictionary) =>
            nextDictionary.settings.status.pressCombination(
              nextDictionary.settings.shortcuts.actions[action].label,
            ),
          "neutral",
        );
      }
    });

    const error = document.createElement("p");
    error.id = `shortcut-error-${action}`;
    error.className = "field-error";
    error.setAttribute("role", "alert");

    copy.append(label, description);
    control.append(input, error);
    row.append(copy, control);
    shortcutList.append(row);
    shortcutElements.set(action, { input, label, description, error, errorText: null });
  }
}

function applyLanguage(language: Language): void {
  currentLanguage = language;
  const copy = dictionary();
  const settingsCopy = copy.settings;

  document.documentElement.lang = language;
  document.title = settingsCopy.windowTitle;
  productName.textContent = copy.common.productName;
  settingsHeading.textContent = settingsCopy.heading;
  settingsSubtitle.textContent = settingsCopy.subtitle;
  applicationHeading.textContent = settingsCopy.application.heading;
  applicationDescription.textContent = settingsCopy.application.description;
  shortcutsHeading.textContent = settingsCopy.shortcuts.heading;
  shortcutsDescription.textContent = settingsCopy.shortcuts.description;
  restoreButton.textContent = settingsCopy.shortcuts.restoreDefaults;

  languageLabel.textContent = settingsCopy.application.language.label;
  languageDescription.textContent = settingsCopy.application.language.description;
  for (const [index, languageCode] of LANGUAGES.entries()) {
    languageSelect.options[index].textContent =
      settingsCopy.application.languageOptions[languageCode];
  }

  for (const setting of APPLICATION_SWITCHES) {
    const elements = switchElements.get(setting);
    if (!elements) continue;
    const translatedSetting = settingsCopy.application[setting];
    elements.label.textContent = translatedSetting.label;
    elements.description.textContent = translatedSetting.description;
    elements.input.setAttribute("aria-label", translatedSetting.label);
  }

  for (const action of SHORTCUT_ACTION_IDS) {
    const elements = shortcutElements.get(action);
    if (!elements) continue;
    const translatedAction = settingsCopy.shortcuts.actions[action];
    elements.label.textContent = translatedAction.label;
    elements.description.textContent = translatedAction.description;
    elements.input.placeholder = settingsCopy.shortcuts.pressShortcut;
    elements.input.setAttribute(
      "aria-label",
      settingsCopy.shortcuts.inputAriaLabel(translatedAction.label),
    );
    renderFieldError(elements);
  }

  renderStatus();
}

async function loadSettings(): Promise<void> {
  setControlsDisabled(true);
  setStatus((copy) => copy.settings.status.loading, "busy");
  try {
    settings = await invoke<AppSettings>("get_settings");
    applyLanguage(isLanguage(settings.application.language) ? settings.application.language : "en");
    displaySettings(settings);
    setControlsDisabled(false);
    setStatus((copy) => copy.settings.status.ready, "success");
  } catch (error: unknown) {
    const detail = formatError(error);
    setStatus((copy) => copy.settings.status.loadFailed(detail), "error");
  } finally {
    applicationList.setAttribute("aria-busy", "false");
    shortcutList.setAttribute("aria-busy", "false");
  }
}

async function updateApplicationSetting<K extends keyof ApplicationSettings>(
  key: K,
  value: ApplicationSettings[K],
): Promise<void> {
  if (!settings || pendingAction !== null || applicationUpdatePending) {
    if (settings) displayApplicationSettings(settings.application);
    return;
  }

  const previous = settings.application;
  const application = { ...previous, [key]: value };
  if (key === "language" && isLanguage(value)) applyLanguage(value);

  applicationUpdatePending = true;
  clearFieldErrors();
  displayApplicationSettings(application);
  setControlsDisabled(true);
  setStatus((copy) => copy.settings.status.savingApplication, "busy");

  try {
    const updated = await invoke<AppSettings>("update_application_settings", { application });
    settings = updated;
    applyLanguage(isLanguage(updated.application.language) ? updated.application.language : "en");
    displaySettings(updated);
    setStatus((copy) => copy.settings.status.applicationSaved, "success");
  } catch (error: unknown) {
    applyLanguage(previous.language);
    displayApplicationSettings(previous);
    const detail = formatError(error);
    setStatus((copy) => copy.settings.status.applicationFailed(detail), "error");
  } finally {
    applicationUpdatePending = false;
    setControlsDisabled(false);
  }
}

async function captureShortcut(action: ShortcutAction, event: KeyboardEvent): Promise<void> {
  event.preventDefault();
  event.stopPropagation();
  if (event.repeat || pendingAction !== null || applicationUpdatePending) return;

  if (event.key === "Escape") {
    shortcutElements.get(action)?.input.blur();
    setFieldError(action, null);
    setStatus((copy) => copy.settings.status.changeCancelled, "neutral");
    return;
  }

  const shortcut = shortcutFromKeyboardEvent(event);
  if (!shortcut) {
    setFieldError(action, (copy) => copy.settings.status.modifierOnly);
    return;
  }
  if (!settings) {
    setFieldError(action, (copy) => copy.settings.status.unavailable);
    return;
  }

  const previousShortcut = settings.shortcuts[action];
  const elements = shortcutElements.get(action);
  if (!elements) return;
  if (shortcutIdentity(shortcut) === shortcutIdentity(previousShortcut)) {
    setFieldError(action, null);
    setStatus((copy) => copy.settings.status.unchanged, "neutral");
    elements.input.blur();
    return;
  }

  const duplicate = SHORTCUT_ACTION_IDS.find(
    (candidate) =>
      candidate !== action &&
      shortcutIdentity(settings?.shortcuts[candidate] ?? "") === shortcutIdentity(shortcut),
  );
  if (duplicate) {
    setFieldError(action, (copy) =>
      copy.settings.status.duplicate(
        shortcut,
        copy.settings.shortcuts.actions[duplicate].label,
      ),
    );
    setStatus((copy) => copy.settings.status.uniqueRequired, "error");
    return;
  }

  pendingAction = action;
  setFieldError(action, null);
  elements.input.value = shortcut;
  setControlsDisabled(true);
  setStatus((copy) => copy.settings.status.applyingShortcut(shortcut), "busy");

  try {
    const updated = await invoke<AppSettings>("update_shortcut", { action, shortcut });
    settings = updated;
    displaySettings(updated);
    setStatus((copy) => copy.settings.status.shortcutApplied(shortcut), "success");
  } catch (error: unknown) {
    elements.input.value = previousShortcut;
    const detail = formatError(error);
    setFieldError(action, () => detail);
    setStatus((copy) => copy.settings.status.shortcutFailed(detail), "error");
  } finally {
    pendingAction = null;
    setControlsDisabled(false);
    elements.input.focus();
    elements.input.select();
  }
}

async function restoreDefaults(): Promise<void> {
  if (pendingAction !== null || applicationUpdatePending) return;
  clearFieldErrors();
  setControlsDisabled(true);
  setStatus((copy) => copy.settings.status.restoringDefaults, "busy");
  try {
    const updated = await invoke<AppSettings>("restore_defaults");
    settings = updated;
    applyLanguage(isLanguage(updated.application.language) ? updated.application.language : "en");
    displaySettings(updated);
    setStatus((copy) => copy.settings.status.defaultsRestored, "success");
  } catch (error: unknown) {
    const detail = formatError(error);
    setStatus((copy) => copy.settings.status.restoreFailed(detail), "error");
  } finally {
    setControlsDisabled(false);
  }
}

function shortcutFromKeyboardEvent(event: KeyboardEvent): string | null {
  if (isModifierKey(event.key)) return null;
  const key = normalizeKey(event);
  if (!key) return null;

  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Super");
  parts.push(key);
  return parts.join("+");
}

function normalizeKey(event: KeyboardEvent): string | null {
  const namedKeys: Readonly<Record<string, string>> = {
    " ": "Space",
    Spacebar: "Space",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    ArrowUp: "Up",
    ArrowDown: "Down",
    Del: "Delete",
  };
  const named = namedKeys[event.key];
  if (named) return named;

  if (/^Key[A-Z]$/.test(event.code)) return event.code.slice(3);
  if (/^Digit[0-9]$/.test(event.code)) return event.code.slice(5);
  if (/^F(?:[1-9]|1[0-9]|2[0-4])$/.test(event.key)) return event.key.toUpperCase();

  const supportedCodes = new Set([
    "Backquote",
    "Backslash",
    "BracketLeft",
    "BracketRight",
    "Comma",
    "Equal",
    "Minus",
    "NumpadAdd",
    "NumpadDecimal",
    "NumpadDivide",
    "NumpadMultiply",
    "NumpadSubtract",
    "Period",
    "Quote",
    "Semicolon",
    "Slash",
  ]);
  if (supportedCodes.has(event.code)) return event.code;

  const supportedKeys = new Set([
    "Backspace",
    "CapsLock",
    "Delete",
    "End",
    "Enter",
    "Home",
    "Insert",
    "NumLock",
    "PageDown",
    "PageUp",
    "Pause",
    "ScrollLock",
    "Tab",
  ]);
  return supportedKeys.has(event.key) ? event.key : null;
}

function isModifierKey(key: string): boolean {
  return ["Alt", "AltGraph", "Control", "Meta", "OS", "Shift"].includes(key);
}

function shortcutIdentity(shortcut: string): string {
  return shortcut
    .split("+")
    .map((part) => part.trim().toLowerCase())
    .sort()
    .join("+");
}

function displaySettings(nextSettings: AppSettings): void {
  displayApplicationSettings(nextSettings.application);
  for (const action of SHORTCUT_ACTION_IDS) {
    const elements = shortcutElements.get(action);
    if (elements) elements.input.value = nextSettings.shortcuts[action];
  }
}

function displayApplicationSettings(application: ApplicationSettings): void {
  languageSelect.value = application.language;
  for (const setting of APPLICATION_SWITCHES) {
    const elements = switchElements.get(setting);
    if (elements) elements.input.checked = application[setting];
  }
}

function setControlsDisabled(disabled: boolean): void {
  restoreButton.disabled = disabled;
  languageSelect.disabled = disabled;
  for (const { input } of switchElements.values()) input.disabled = disabled;
  for (const { input } of shortcutElements.values()) input.disabled = disabled;
}

function clearFieldErrors(): void {
  for (const action of SHORTCUT_ACTION_IDS) setFieldError(action, null);
}

function setFieldError(action: ShortcutAction, message: LocalizedText | null): void {
  const elements = shortcutElements.get(action);
  if (!elements) return;
  elements.errorText = message;
  renderFieldError(elements);
}

function renderFieldError(elements: ShortcutElements): void {
  const message = elements.errorText?.(dictionary()) ?? "";
  elements.error.textContent = message;
  elements.input.classList.toggle("has-error", message.length > 0);
  elements.input.setAttribute("aria-invalid", String(message.length > 0));
}

function setStatus(message: LocalizedText, kind: StatusKind): void {
  currentStatusText = message;
  currentStatusKind = kind;
  renderStatus();
}

function renderStatus(): void {
  statusMessage.textContent = currentStatusText(dictionary());
  statusIndicator.className = `status-indicator status-${currentStatusKind}`;
}

function formatError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "message" in error) {
    return String(error.message);
  }
  return dictionary().common.unexpectedError;
}

renderApplicationFields();
renderShortcutFields();
applyLanguage(currentLanguage);
restoreButton.addEventListener("click", () => void restoreDefaults());
void loadSettings();
