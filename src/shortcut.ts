// Converting between browser key events, Tauri accelerator strings
// ("Alt+Space") and macOS-style labels ("⌥ Space").

const MODIFIER_CODES = new Set([
  "ShiftLeft",
  "ShiftRight",
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
  "CapsLock",
  "Fn",
]);

const KEY_LABELS: Record<string, string> = {
  Space: "Space",
  Enter: "↩",
  Backspace: "⌫",
  Tab: "⇥",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
};

function keyName(code: string): string {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  return code;
}

export type CaptureResult = { accelerator: string } | { error: string } | null;

/** Build an accelerator from a keydown event, or null while only modifiers are held. */
export function acceleratorFromEvent(e: KeyboardEvent): CaptureResult {
  if (MODIFIER_CODES.has(e.code)) return null;
  if (e.code === "Escape") return { error: "Escape is reserved for cancelling" };
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Control");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Super");
  const isFunctionKey = /^F\d{1,2}$/.test(e.code);
  if (parts.length === 0 && !isFunctionKey) {
    return { error: "Add a modifier (⌃ ⌥ ⇧ ⌘), or use a function key" };
  }
  parts.push(keyName(e.code));
  return { accelerator: parts.join("+") };
}

/** "Alt+Shift+KeyD" → "⌥ ⇧ D" */
export function formatAccelerator(accelerator: string): string {
  return accelerator
    .split("+")
    .map((part) => {
      switch (part.toLowerCase()) {
        case "control":
        case "ctrl":
          return "⌃";
        case "alt":
        case "option":
          return "⌥";
        case "shift":
          return "⇧";
        case "super":
        case "cmd":
        case "command":
        case "meta":
        case "commandorcontrol":
        case "cmdorctrl":
          return "⌘";
        default: {
          const name = keyName(part);
          return KEY_LABELS[name] ?? name;
        }
      }
    })
    .join(" ");
}
