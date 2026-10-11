import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { Fragment, useEffect, useState } from "react";
import { Toaster } from "sonner";
import { HostKeyPrompt } from "@/components/host-key-prompt";
import { currentLang, onLangChange } from "@/lib/i18n";
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
  router = createAppRouter(),
  queryClient,
}: {
  router?: ReturnType<typeof createAppRouter>;
  queryClient?: QueryClient;
}) {
  const [client] = useState(() => queryClient ?? createQueryClient());
  // A new language renders everything again; the backend's data are messages, so nothing is fetched again.
  const [lang, setLang] = useState(currentLang);
  useEffect(() => onLangChange(setLang), []);
  return (
    <QueryClientProvider client={client}>
      <Fragment key={lang}>
        <RouterProvider router={router} />
        <Toaster theme="dark" position="bottom-right" />
        <HostKeyPrompt />
      </Fragment>
    </QueryClientProvider>
  );
}
