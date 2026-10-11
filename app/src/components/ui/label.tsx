import { Label as LabelPrimitive } from "radix-ui";
import type * as React from "react";
import { cn } from "@/lib/utils";

export function Label({ className, ...props }: React.ComponentProps<typeof LabelPrimitive.Root>) {
  return <LabelPrimitive.Root className={cn("text-[13px] font-medium", className)} {...props} />;
}

export function FieldHint({ className, ...props }: React.ComponentProps<"p">) {
  return <p className={cn("text-xs text-muted-foreground", className)} {...props} />;
}

/**
 * Reserves one line for a validation message, so the layout does not jump
 * when the message appears or disappears.
 */
export function ErrorSlot({ message, className }: { message?: string | null | false; className?: string }) {
  return (
    <div aria-live="polite" className={cn("min-h-[18px]", className)}>
      {message ? <FieldError>{message}</FieldError> : null}
    </div>
  );
}

export function FieldError({ className, ...props }: React.ComponentProps<"p">) {
  return <p role="alert" className={cn("text-xs text-red-300", className)} {...props} />;
}
