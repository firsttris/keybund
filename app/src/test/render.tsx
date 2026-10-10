import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App, createQueryClient } from "@/app";
import type { MockBackend } from "@/lib/mock/backend";
import { installMockBackend } from "@/lib/mock/install";
import { createAppRouter } from "@/router";

/**
 * Renders the whole app at `path` against a fresh mock backend; `setup`
 * can change the backend's state before the first request.
 */
export async function renderApp(path = "/", setup?: (backend: MockBackend) => void) {
  const backend = installMockBackend({ latency: 0, openTabs: false });
  setup?.(backend);
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  const user = userEvent.setup();
  const utils = render(<App router={router} queryClient={createQueryClient()} />);
  await screen.findByRole("navigation", { name: "Profile" }).catch(() => undefined);
  return { ...utils, backend, router, user };
}
