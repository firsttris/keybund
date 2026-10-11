/**
 * The UI language. Texts live in messages/{de,en}.json and are compiled by
 * Paraglide to src/paraglide: components call them directly (`m.home_title()`).
 * The backend never sends text, only messages – a key and its arguments – which
 * `t()` renders here, so errors, doctor checks and test steps follow the UI
 * language too.
 *
 * The language comes from the setting (localStorage), else from the system:
 * German for de-*, English for everything else.
 */
import { createElement, Fragment, type ReactNode } from "react";
import { m } from "@/paraglide/messages";
import { overwriteGetLocale } from "@/paraglide/runtime";
import type { Msg } from "./bindings";

export type Lang = "de" | "en";
export const LANGS: { id: Lang; label: string }[] = [
  { id: "de", label: "Deutsch" },
  { id: "en", label: "English" },
];

const STORAGE_KEY = "keybund_lang";

const isLang = (v: unknown): v is Lang => v === "de" || v === "en";

/** The stored choice, `null` = follow the system. */
export function storedLang(): Lang | null {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    return isLang(v) ? v : null;
  } catch {
    return null;
  }
}

/** The language of the system: German for de-*, English otherwise. */
export function systemLang(languages: readonly string[] = navigator.languages ?? [navigator.language]): Lang {
  const first = languages.find(Boolean) ?? "en";
  return first.toLowerCase().startsWith("de") ? "de" : "en";
}

let current: Lang | undefined;

export function currentLang(): Lang {
  current ??= storedLang() ?? systemLang();
  return current;
}

overwriteGetLocale(() => currentLang());

const listeners = new Set<(lang: Lang) => void>();

/** Called when the language changes (the app re-renders with the new texts). */
export function onLangChange(fn: (lang: Lang) => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Switches the language; `null` follows the system again. */
export function setLang(lang: Lang | null) {
  try {
    if (lang) localStorage.setItem(STORAGE_KEY, lang);
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Without storage the choice lasts until the app closes.
  }
  current = lang ?? systemLang();
  document.documentElement.lang = current;
  for (const fn of listeners) fn(current);
}

/** Intl locale for dates and times. */
export const localeOf = (lang: Lang = currentLang()) => (lang === "en" ? "en-GB" : "de-DE");

type MessageFn = (inputs?: Record<string, string>) => string;
const messages = m as unknown as Record<string, MessageFn | undefined>;

/** Renders a message from the backend (nested messages included); unknown keys come back as they are. */
export function t(msg: Msg): string {
  const fn = messages[msg.key];
  if (!fn) return msg.key;
  const inputs: Record<string, string> = {};
  for (const [name, value] of Object.entries(msg.args)) {
    if (value !== undefined) inputs[name] = typeof value === "string" ? value : t(value);
  }
  return fn(inputs);
}

/** A message chosen by a runtime key (provider, colour, status …); unknown keys come back as they are. */
export function pick(map: Record<string, () => string>, key: string): string {
  return map[key]?.() ?? key;
}

/**
 * A message with React elements in it: the message gets the parameter names
 * as placeholders, the text around them stays plain, the elements go in between.
 *   rich(m.import_intro, { file: <code>~/.ssh/config</code> })
 */
export function rich<P extends Record<string, ReactNode>>(
  message: (inputs: Record<keyof P, string>) => string,
  parts: P,
): ReactNode {
  const marks = Object.fromEntries(Object.keys(parts).map((k) => [k, `\uE000${k}\uE000`])) as Record<keyof P, string>;
  const pieces = message(marks).split(/\uE000(\w+)\uE000/);
  return createElement(
    Fragment,
    null,
    ...pieces.map((p, i) => (i % 2 ? createElement(Fragment, { key: i }, parts[p as keyof P]) : p)),
  );
}
