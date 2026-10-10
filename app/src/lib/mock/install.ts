import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { createMockBackend, type MockBackend } from "./backend";

declare global {
  interface Window {
    __keybundMock?: MockBackend;
  }
}

/** Routes all Tauri IPC calls to a fresh in-memory backend. */
export function installMockBackend(options: { latency?: number; openTabs?: boolean } = {}): MockBackend {
  const backend = createMockBackend();
  if (options.latency !== undefined) backend.state.latency = options.latency;
  // In the browser preview links really open; under Playwright they are only recorded.
  backend.state.openTabs = options.openTabs ?? !navigator.webdriver;
  mockWindows("main");
  mockIPC((cmd, args) => backend.invoke(cmd, (args ?? {}) as Record<string, unknown>));
  window.__keybundMock = backend;
  return backend;
}
