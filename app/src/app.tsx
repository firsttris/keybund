import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { Fragment, useEffect, useState, useSyncExternalStore } from "react";
import { Toaster } from "sonner";
import { HostKeyPrompt } from "@/components/host-key-prompt";
import { currentLang, onLangChange } from "@/lib/i18n";
import { appliedTheme, onThemeChange } from "@/lib/theme";
import { createAppRouter } from "./router";

export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, refetchOnWindowFocus: true, staleTime: 5_000 },
      mutations: { retry: false },
    },
  });
}

export function App({
  router: givenRouter,
  queryClient,
}: {
  router?: ReturnType<typeof createAppRouter>;
  queryClient?: QueryClient;
}) {
  // Once per app: a new router on a re-render (theme or language) would start empty.
  const [router] = useState(() => givenRouter ?? createAppRouter());
  const [client] = useState(() => queryClient ?? createQueryClient());
  // A new language renders everything again; the backend's data are messages, so nothing is fetched again.
  const [lang, setLang] = useState(currentLang);
  useEffect(() => onLangChange(setLang), []);
  const theme = useSyncExternalStore(onThemeChange, appliedTheme);
  return (
    <QueryClientProvider client={client}>
      <Fragment key={lang}>
        <RouterProvider router={router} />
        <Toaster theme={theme} position="bottom-right" />
        <HostKeyPrompt />
      </Fragment>
    </QueryClientProvider>
  );
}
