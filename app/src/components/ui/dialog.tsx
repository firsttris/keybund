import { AlertDialog as Primitive } from "radix-ui";
import type * as React from "react";
import { cn } from "@/lib/utils";
import { buttonVariants } from "./button";

export const AlertDialog = Primitive.Root;
export const AlertDialogTrigger = Primitive.Trigger;

export function AlertDialogContent({ className, children, ...props }: React.ComponentProps<typeof Primitive.Content>) {
  return (
    <Primitive.Portal>
      <Primitive.Overlay className="fixed inset-0 z-50 bg-black/40 dark:bg-black/60" />
      <Primitive.Content
        className={cn(
          "fixed top-1/2 left-1/2 z-50 flex w-[min(440px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col gap-4 rounded-xl border border-border bg-card p-6 text-foreground",
          className,
        )}
        {...props}
      >
        {children}
      </Primitive.Content>
    </Primitive.Portal>
  );
}

export function AlertDialogTitle({ className, ...props }: React.ComponentProps<typeof Primitive.Title>) {
  return <Primitive.Title className={cn("text-base font-semibold", className)} {...props} />;
}

export function AlertDialogDescription({ className, ...props }: React.ComponentProps<typeof Primitive.Description>) {
  return (
    <Primitive.Description className={cn("text-[13px] leading-relaxed text-muted-foreground", className)} {...props} />
  );
}

export function AlertDialogFooter({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("flex flex-wrap justify-end gap-2", className)} {...props} />;
}

export function AlertDialogCancel({ className, ...props }: React.ComponentProps<typeof Primitive.Cancel>) {
  return <Primitive.Cancel className={cn(buttonVariants({ variant: "outline" }), className)} {...props} />;
}

export function AlertDialogAction({ className, ...props }: React.ComponentProps<typeof Primitive.Action>) {
  return <Primitive.Action className={cn(buttonVariants({ variant: "default" }), className)} {...props} />;
}
