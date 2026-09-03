import { invoke } from "@tauri-apps/api/core";

const SHORTCUT_ACTIONS = [
  { id: "playPause", label: "Play / Pause", description: "Toggle playback" },
  { id: "next", label: "Next", description: "Play the next track" },
  { id: "previous", label: "Previous", description: "Play the previous track" },
  { id: "seekForward10", label: "Seek Forward 10s", description: "Skip ahead ten seconds" },
  { id: "seekBackward10", label: "Seek Backward 10s", description: "Skip back ten seconds" },
] as const;

type ShortcutAction = (typeof SHORTCUT_ACTIONS)[number]["id"];

interface ShortcutSettings {
  playPause: string;
  next: string;
  previous: string;
  seekForward10: string;
  seekBackward10: string;
}

interface AppSettings {
  shortcuts: ShortcutSettings;
}

interface ShortcutElements {
  input: HTMLInputElement;
  error: HTMLParagraphElement;
}

const shortcutElements = new Map<ShortcutAction, ShortcutElements>();
let settings: AppSettings | null = null;
let pendingAction: ShortcutAction | null = null;

const shortcutList = requireElement<HTMLDivElement>("#shortcut-list");
const restoreButton = requireElement<HTMLButtonElement>("#restore-defaults");
const statusMessage = requireElement<HTMLParagraphElement>("#status-message");
const statusIndicator = requireElement<HTMLSpanElement>("#status-indicator");

function requireElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`Required settings element is missing: ${selector}`);
  return element;
}

function renderShortcutFields(): void {
  for (const action of SHORTCUT_ACTIONS) {
    const row = document.createElement("div");
    row.className = "shortcut-row";

    const copy = document.createElement("div");
    copy.className = "shortcut-copy";
    const label = document.createElement("label");
    label.htmlFor = `shortcut-${action.id}`;
    label.textContent = action.label;
    const description = document.createElement("p");
    description.textContent = action.description;

    const control = document.createElement("div");
    control.className = "shortcut-control";
    const input = document.createElement("input");
    input.id = `shortcut-${action.id}`;
    input.className = "shortcut-input";
    input.type = "text";
    input.readOnly = true;
    input.autocomplete = "off";
    input.spellcheck = false;
    input.placeholder = "Press shortcut";
    input.setAttribute("aria-describedby", `shortcut-error-${action.id}`);
    input.setAttribute("aria-label", `${action.label} shortcut`);
    input.addEventListener("keydown", (event) => void captureShortcut(action.id, event));
    input.addEventListener("focus", () => {
      if (pendingAction === null) {
        setStatus(`Press a key combination for ${action.label}.`, "neutral");
      }
    });

    const error = document.createElement("p");
    error.id = `shortcut-error-${action.id}`;
    error.className = "field-error";
    error.setAttribute("role", "alert");

    copy.append(label, description);
    control.append(input, error);
    row.append(copy, control);
    shortcutList.append(row);
    shortcutElements.set(action.id, { input, error });
  }
}

async function loadSettings(): Promise<void> {
  setControlsDisabled(true);
  setStatus("Loading settings…", "busy");
  try {
    settings = await invoke<AppSettings>("get_settings");
    displaySettings(settings);
    setControlsDisabled(false);
    setStatus("Settings are up to date.", "success");
  } catch (error: unknown) {
    setStatus(`Could not load settings: ${formatError(error)}`, "error");
  } finally {
    shortcutList.setAttribute("aria-busy", "false");
  }
}

async function captureShortcut(action: ShortcutAction, event: KeyboardEvent): Promise<void> {
  event.preventDefault();
  event.stopPropagation();
  if (event.repeat || pendingAction !== null) return;

  if (event.key === "Escape") {
    shortcutElements.get(action)?.input.blur();
    setFieldError(action, "");
    setStatus("Shortcut change cancelled.", "neutral");
    return;
  }

  const shortcut = shortcutFromKeyboardEvent(event);
  if (!shortcut) {
    setFieldError(action, "Press a non-modifier key with any modifiers you want.");
    return;
  }
  if (!settings) {
    setFieldError(action, "Settings are not available yet.");
    return;
  }

  const previousShortcut = settings.shortcuts[action];
  const elements = shortcutElements.get(action);
  if (!elements) return;
  if (shortcutIdentity(shortcut) === shortcutIdentity(previousShortcut)) {
    setFieldError(action, "");
    setStatus("Shortcut is unchanged.", "neutral");
    elements.input.blur();
    return;
  }

  const duplicate = SHORTCUT_ACTIONS.find(
    (candidate) =>
      candidate.id !== action &&
      shortcutIdentity(settings?.shortcuts[candidate.id] ?? "") === shortcutIdentity(shortcut),
  );
  if (duplicate) {
    setFieldError(action, `${shortcut} is already assigned to ${duplicate.label}.`);
    setStatus("Each action needs a unique shortcut.", "error");
    return;
  }

  pendingAction = action;
  setFieldError(action, "");
  elements.input.value = shortcut;
  setControlsDisabled(true);
  setStatus(`Applying ${shortcut}…`, "busy");

  try {
    const updated = await invoke<AppSettings>("update_shortcut", { action, shortcut });
    settings = updated;
    displaySettings(updated);
    setStatus(`${shortcut} is ready to use.`, "success");
  } catch (error: unknown) {
    elements.input.value = previousShortcut;
    const message = formatError(error);
    setFieldError(action, message);
    setStatus(`Could not change the shortcut: ${message}`, "error");
  } finally {
    pendingAction = null;
    setControlsDisabled(false);
    elements.input.focus();
    elements.input.select();
  }
}

async function restoreDefaults(): Promise<void> {
  if (pendingAction !== null) return;
  clearFieldErrors();
  setControlsDisabled(true);
  setStatus("Restoring default shortcuts…", "busy");
  try {
    const updated = await invoke<AppSettings>("restore_default_shortcuts");
    settings = updated;
    displaySettings(updated);
    setStatus("Default shortcuts restored.", "success");
  } catch (error: unknown) {
    setStatus(`Could not restore defaults: ${formatError(error)}`, "error");
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
  for (const action of SHORTCUT_ACTIONS) {
    const elements = shortcutElements.get(action.id);
    if (elements) elements.input.value = nextSettings.shortcuts[action.id];
  }
}

function setControlsDisabled(disabled: boolean): void {
  restoreButton.disabled = disabled;
  for (const { input } of shortcutElements.values()) input.disabled = disabled;
}

function clearFieldErrors(): void {
  for (const action of SHORTCUT_ACTIONS) setFieldError(action.id, "");
}

function setFieldError(action: ShortcutAction, message: string): void {
  const elements = shortcutElements.get(action);
  if (!elements) return;
  elements.error.textContent = message;
  elements.input.classList.toggle("has-error", message.length > 0);
  elements.input.setAttribute("aria-invalid", String(message.length > 0));
}

function setStatus(message: string, kind: "neutral" | "busy" | "success" | "error"): void {
  statusMessage.textContent = message;
  statusIndicator.className = `status-indicator status-${kind}`;
}

function formatError(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "message" in error) {
    return String(error.message);
  }
  return "An unexpected error occurred.";
}

renderShortcutFields();
restoreButton.addEventListener("click", () => void restoreDefaults());
void loadSettings();
