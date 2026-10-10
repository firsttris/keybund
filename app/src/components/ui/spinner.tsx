import { cn } from "@/lib/utils";

export function Spinner({ className, label = "Lädt …" }: { className?: string; label?: string }) {
  return (
    <span
      role="status"
      aria-label={label}
      className={cn("inline-block size-4 animate-[spin_0.8s_linear_infinite] rounded-full border-2 border-border border-t-foreground", className)}
    />
  );
}
