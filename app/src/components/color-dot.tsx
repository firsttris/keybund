import { cn, colorHex } from "@/lib/utils";

export function ColorDot({ color, className }: { color: string | null | undefined; className?: string }) {
  return (
    <span
      aria-hidden
      data-color={color ?? "zinc"}
      className={cn("inline-block size-2 shrink-0 rounded-full", className)}
      style={{ background: colorHex(color) }}
    />
  );
}
