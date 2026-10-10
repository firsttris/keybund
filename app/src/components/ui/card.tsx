import * as React from "react";
import { cn } from "@/lib/utils";

export function Card({ className, ...props }: React.ComponentProps<"section">) {
  return (
    <section
      className={cn("flex flex-col gap-4 rounded-xl border border-border bg-card p-5", className)}
      {...props}
    />
  );
}

export function CardHeader({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("flex flex-wrap items-center justify-between gap-3", className)} {...props} />;
}

export function CardTitle({ className, ...props }: React.ComponentProps<"h2">) {
  return <h2 className={cn("m-0 text-[15px] font-semibold", className)} {...props} />;
}

/** A dark row inside a card, used for hosts and folders. */
export function ItemRow({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("flex items-center gap-3 rounded-lg bg-muted p-3", className)} {...props} />;
}
