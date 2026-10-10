import { Outlet } from "@tanstack/react-router";
import { Sidebar } from "./sidebar";

export function AppShell() {
  return (
    <div className="flex min-h-full flex-wrap bg-background">
      <Sidebar />
      <main className="flex min-w-0 flex-[999_1_560px] flex-col gap-6 px-8 pt-7 pb-6 max-[640px]:px-4">
        <Outlet />
      </main>
    </div>
  );
}

/** Full-width page with a back link, used for the wizard and "Welcher Key?". */
export function PageFrame({ back, aside, children }: { back: React.ReactNode; aside?: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="flex min-h-full flex-col bg-background">
      <div className="flex items-center justify-between gap-4 border-b border-border px-7 py-3.5">
        {back}
        {aside}
      </div>
      <div className="flex flex-1 justify-center px-6 py-9 max-[640px]:px-4">{children}</div>
    </div>
  );
}
