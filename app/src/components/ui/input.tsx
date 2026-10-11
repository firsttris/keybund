import type * as React from "react";
import { cn } from "@/lib/utils";

export function Input({ className, type = "text", ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(
        "h-[38px] w-full min-w-0 rounded-lg border border-input bg-background px-3 text-[13px] text-foreground placeholder:text-muted-foreground disabled:opacity-50 aria-[invalid=true]:border-red-400/70",
        className,
      )}
      {...props}
    />
  );
}
