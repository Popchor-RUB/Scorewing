import { Combobox as ComboboxPrimitive } from "@base-ui/react/combobox";

// shadcn/ui Base UI anatomy, styled with the desktop's shared semantic tokens.
export const Combobox = ComboboxPrimitive.Root;
export const ComboboxCollection = ComboboxPrimitive.Collection;

export function ComboboxInput({ className, disabled, ...props }: ComboboxPrimitive.Input.Props) {
  return <ComboboxPrimitive.InputGroup data-slot="combobox-input-group" className="combobox-input-group">
    <ComboboxPrimitive.Input data-slot="combobox-input" className={["input combobox-input", className].filter(Boolean).join(" ")} disabled={disabled} {...props} />
    <ComboboxPrimitive.Trigger data-slot="combobox-trigger" className="combobox-trigger" disabled={disabled} aria-label="Show suggestions">
      <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6" /></svg>
    </ComboboxPrimitive.Trigger>
  </ComboboxPrimitive.InputGroup>;
}

export function ComboboxContent({ className, children, ...props }: ComboboxPrimitive.Popup.Props) {
  return <ComboboxPrimitive.Portal><ComboboxPrimitive.Positioner sideOffset={4} align="start" className="popup-positioner combobox-positioner">
    <ComboboxPrimitive.Popup data-slot="combobox-content" className={["combobox-content", className].filter(Boolean).join(" ")} {...props}>{children}</ComboboxPrimitive.Popup>
  </ComboboxPrimitive.Positioner></ComboboxPrimitive.Portal>;
}

export function ComboboxList({ className, ...props }: ComboboxPrimitive.List.Props) {
  return <ComboboxPrimitive.List data-slot="combobox-list" className={["combobox-list", className].filter(Boolean).join(" ")} {...props} />;
}

export function ComboboxItem({ className, children, ...props }: ComboboxPrimitive.Item.Props) {
  return <ComboboxPrimitive.Item data-slot="combobox-item" className={["combobox-item", className].filter(Boolean).join(" ")} {...props}>
    {children}<ComboboxPrimitive.ItemIndicator aria-hidden="true">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m20 6-11 11-5-5" /></svg>
    </ComboboxPrimitive.ItemIndicator>
  </ComboboxPrimitive.Item>;
}

export function ComboboxGroup({ className, ...props }: ComboboxPrimitive.Group.Props) {
  return <ComboboxPrimitive.Group data-slot="combobox-group" className={["combobox-group", className].filter(Boolean).join(" ")} {...props} />;
}

export function ComboboxLabel({ className, ...props }: ComboboxPrimitive.GroupLabel.Props) {
  return <ComboboxPrimitive.GroupLabel data-slot="combobox-label" className={["combobox-label", className].filter(Boolean).join(" ")} {...props} />;
}

export function ComboboxEmpty({ className, ...props }: ComboboxPrimitive.Empty.Props) {
  return <ComboboxPrimitive.Empty data-slot="combobox-empty" className={["combobox-empty", className].filter(Boolean).join(" ")} {...props} />;
}

export function ComboboxSeparator({ className, ...props }: ComboboxPrimitive.Separator.Props) {
  return <ComboboxPrimitive.Separator data-slot="combobox-separator" className={["combobox-separator", className].filter(Boolean).join(" ")} {...props} />;
}
