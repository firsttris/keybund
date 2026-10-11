import { m } from "@/paraglide/messages";
import { commands, type CmdError, type Msg } from "./bindings";
import { t } from "./i18n";

/** Error thrown by every command wrapper; carries the backend's kind. */
export class ApiError extends Error {
  kind: string;
  /** The backend's message; `message` is its text in the current language. */
  msg: Msg;
  constructor(error: CmdError) {
    super(t(error.message));
    this.name = "ApiError";
    this.kind = error.kind;
    this.msg = error.message;
  }
}

type Result<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function unwrap<T>(promise: Promise<Result<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new ApiError(result.error);
  return result.data;
}

type Commands = typeof commands;
type Unwrapped = {
  [K in keyof Commands]: Commands[K] extends (...args: infer A) => Promise<Result<infer T>>
    ? (...args: A) => Promise<T>
    : Commands[K];
};

/** The typed commands, throwing [`ApiError`] instead of returning results. */
export const api = new Proxy({} as Unwrapped, {
  get(_target, name: string) {
    const fn = (commands as Record<string, (...args: unknown[]) => Promise<unknown>>)[name];
    return async (...args: unknown[]) => {
      const value = await fn(...args);
      if (value && typeof value === "object" && "status" in value && (value.status === "ok" || value.status === "error")) {
        return unwrap(Promise.resolve(value as Result<unknown>));
      }
      return value;
    };
  },
});

export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return m.error_unknown();
}
