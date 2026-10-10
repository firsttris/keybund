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
import { NewProjectPage } from "./routes/new-project";
import { ProfilePage } from "./routes/profile";
import { SettingsPage } from "./routes/settings";
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

const newRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/new",
  validateSearch: (search: Record<string, unknown>): { key?: string } =>
    typeof search.key === "string" ? { key: search.key } : {},
  component: function New() {
    const { key } = newRoute.useSearch();
    return <NewProjectPage existingKey={key} />;
  },
});

const whichRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/which",
  validateSearch: (search: Record<string, unknown>): { q?: string } =>
    typeof search.q === "string" ? { q: search.q } : {},
  component: function Which() {
    const { q } = whichRoute.useSearch();
    return <WhichPage initial={q} />;
  },
});

const routeTree = rootRoute.addChildren([
  shell.addChildren([homeRoute, profileRoute, applyRoute, cloneRoute, doctorRoute, settingsRoute]),
  newRoute,
  whichRoute,
]);

export function createAppRouter(history: RouterHistory = createHashHistory()) {
  return createRouter({ routeTree, history, defaultPreload: false });
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
