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
