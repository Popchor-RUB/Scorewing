import { Button as ButtonPrimitive } from "@base-ui/react/button";

// shadcn/ui Base UI anatomy, styled with the desktop's shared semantic tokens.
export function Button({ className, variant = "default", size = "default", ...props }: ButtonPrimitive.Props & {
  variant?: "default" | "outline" | "secondary" | "ghost" | "destructive";
  size?: "default" | "sm" | "icon";
}) {
  return <ButtonPrimitive data-slot="button" data-variant={variant} data-size={size}
    className={["button", className].filter(Boolean).join(" ")} {...props} />;
}
