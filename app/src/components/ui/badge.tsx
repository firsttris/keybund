import { cva, type VariantProps } from "class-variance-authority";
import type * as React from "react";
import { cn } from "@/lib/utils";

const badgeVariants = cva("inline-flex items-center gap-1.5 rounded-md px-2 py-0.5 text-xs font-medium", {
  variants: {
    variant: {
      outline: "border border-border text-secondary-foreground",
      success: "bg-emerald-500/15 text-success",
      warning: "bg-amber-500/15 text-warning",
      danger: "bg-red-500/15 text-danger",
      muted: "bg-muted text-muted-foreground",
    },
  },
  defaultVariants: { variant: "outline" },
});

export function Badge({
  className,
  variant,
  ...props
}: React.ComponentProps<"span"> & VariantProps<typeof badgeVariants>) {
  return <span className={cn(badgeVariants({ variant }), className)} {...props} />;
}
