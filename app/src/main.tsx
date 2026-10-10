import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles.css";

async function start() {
  // Outside of Tauri (browser preview, E2E tests) all commands go to the
  // in-memory mock backend.
  const inTauri = "__TAURI_INTERNALS__" in window && !import.meta.env.VITE_MOCK;
  if (!inTauri) {
    const { installMockBackend } = await import("./lib/mock/install");
    installMockBackend();
  }
  const { App } = await import("./app");
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

void start();
