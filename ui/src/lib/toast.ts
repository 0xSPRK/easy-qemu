import { writable } from "svelte/store";

export type ToastKind = "info" | "success" | "error";

export interface Toast {
  id: number;
  kind: ToastKind;
  text: string;
}

let nextId = 1;

export const toasts = writable<Toast[]>([]);

export function toast(text: string, kind: ToastKind = "info") {
  const t: Toast = { id: nextId++, kind, text };
  toasts.update((list) => [...list, t]);
  const ttl = kind === "error" ? 6000 : 3000;
  setTimeout(() => dismiss(t.id), ttl);
}

export function dismiss(id: number) {
  toasts.update((list) => list.filter((t) => t.id !== id));
}
