import { Dialog as DialogPrimitive } from "@base-ui/react/dialog";

export const Dialog = DialogPrimitive.Root;
export const DialogTrigger = DialogPrimitive.Trigger;
export const DialogClose = DialogPrimitive.Close;
export const DialogTitle = DialogPrimitive.Title;
export const DialogDescription = DialogPrimitive.Description;
export function DialogContent({ className, children, ...props }: DialogPrimitive.Popup.Props) {
  return <DialogPrimitive.Portal><DialogPrimitive.Backdrop data-slot="dialog-overlay" className="dialog-overlay" />
    <DialogPrimitive.Popup data-slot="dialog-content" className={["dialog-content", className].filter(Boolean).join(" ")} {...props}>{children}</DialogPrimitive.Popup>
  </DialogPrimitive.Portal>;
}
