// Global keyboard shortcuts for the main window.
// Kept framework-free so the logic stays trivially testable.

export type HotkeyAction = "new-vm" | "focus-filter" | "delete-selected";

/**
 * Maps a keydown event to a global action, or null when the event is not
 * a shortcut. `modalOpen` disables all of them. Typing in form controls
 * suppresses single-key shortcuts but never Ctrl+F.
 */
export function matchGlobalHotkey(
  e: KeyboardEvent,
  ctx: { modalOpen: boolean }
): HotkeyAction | null {
  if (ctx.modalOpen) return null;

  const t = e.target as HTMLElement | null;
  const typing =
    t instanceof HTMLInputElement ||
    t instanceof HTMLTextAreaElement ||
    t instanceof HTMLSelectElement;

  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
    e.preventDefault();
    return "focus-filter";
  }
  if (typing || e.altKey) return null;

  if (!e.ctrlKey && !e.metaKey && e.key.toLowerCase() === "n") {
    e.preventDefault();
    return "new-vm";
  }
  if (e.key === "Delete") {
    return "delete-selected";
  }
  return null;
}
