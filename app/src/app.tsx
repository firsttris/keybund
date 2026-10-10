import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { useState } from "react";
import { Toaster } from "sonner";
import { HostKeyPrompt } from "@/components/host-key-prompt";
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
  return (
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
      <Toaster theme="dark" position="bottom-right" />
      <HostKeyPrompt />
    </QueryClientProvider>
  );
}
