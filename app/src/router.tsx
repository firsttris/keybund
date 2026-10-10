import {
  createHashHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  type RouterHistory,
} from "@tanstack/react-router";
import { AppShell } from "./components/app-shell";
import { ApplyPage } from "./routes/apply";
import { ClonePage } from "./routes/clone";
import { DoctorPage } from "./routes/doctor";
import { HomePage } from "./routes/home";
import { ImportPage } from "./routes/import";
import { NewKeyPage } from "./routes/new-key";
import { ProfilePage } from "./routes/profile";
import { SettingsPage } from "./routes/settings";
import { UninstallPage } from "./routes/uninstall";
import { WhichPage } from "./routes/which";

const rootRoute = createRootRoute({ component: Outlet });

const shell = createRoute({ getParentRoute: () => rootRoute, id: "shell", component: AppShell });

const homeRoute = createRoute({ getParentRoute: () => shell, path: "/", component: HomePage });

const profileRoute = createRoute({
  getParentRoute: () => shell,
  path: "/profile/$id",
  component: function Profile() {
    const { id } = profileRoute.useParams();
    return <ProfilePage id={id} />;
  },
});

const applyRoute = createRoute({ getParentRoute: () => shell, path: "/apply", component: ApplyPage });
const cloneRoute = createRoute({ getParentRoute: () => shell, path: "/clone", component: ClonePage });
const doctorRoute = createRoute({ getParentRoute: () => shell, path: "/doctor", component: DoctorPage });
const settingsRoute = createRoute({ getParentRoute: () => shell, path: "/settings", component: SettingsPage });
const importRoute = createRoute({ getParentRoute: () => shell, path: "/import", component: ImportPage });
const uninstallRoute = createRoute({ getParentRoute: () => shell, path: "/uninstall", component: UninstallPage });

const newRoute = createRoute({
  getParentRoute: () => shell,
  path: "/new",
  validateSearch: (search: Record<string, unknown>): { key?: string } =>
    typeof search.key === "string" ? { key: search.key } : {},
  component: function New() {
    const { key } = newRoute.useSearch();
    // From "Als Key übernehmen" to "Neuer Key" starts a fresh form.
    return <NewKeyPage key={key ?? ""} existingKey={key} />;
  },
});

const whichRoute = createRoute({
  getParentRoute: () => shell,
  path: "/which",
  validateSearch: (search: Record<string, unknown>): { q?: string } =>
    typeof search.q === "string" ? { q: search.q } : {},
  component: function Which() {
    const { q } = whichRoute.useSearch();
    // A new query from the sidebar search starts a fresh page.
    return <WhichPage key={q} initial={q} />;
  },
});

const routeTree = rootRoute.addChildren([
  shell.addChildren([
    homeRoute,
    profileRoute,
    newRoute,
    applyRoute,
    cloneRoute,
    doctorRoute,
    settingsRoute,
    importRoute,
    uninstallRoute,
    whichRoute,
  ]),
]);

export function createAppRouter(history: RouterHistory = createHashHistory()) {
  return createRouter({ routeTree, history, defaultPreload: false });
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
