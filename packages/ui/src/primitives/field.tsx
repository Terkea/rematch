"use client";

import { WarningCircle } from "@phosphor-icons/react/dist/ssr";
import {
  Children,
  cloneElement,
  isValidElement,
  type ReactElement,
  type ReactNode,
  useId,
} from "react";
import { Icon } from "../icons/icon.js";
import { cn } from "../lib/cn.js";

export interface FieldProps {
  label: string;
  hint?: string;
  error?: string;
  className?: string;
  /** One control (an Input, say). It receives the id, aria-describedby and aria-invalid. */
  children: ReactNode;
}

interface ControlProps {
  id?: string;
  "aria-describedby"?: string;
  "aria-invalid"?: boolean | "true" | "false";
}

/**
 * A labelled control with an optional hint and error (DESIGN.md, "Semantic states"). The error is
 * `pen-text` under the field with Phosphor `WarningCircle`; the control gets `aria-invalid` and is
 * described by the hint, then the error.
 */
export function Field({ label, hint, error, className, children }: FieldProps) {
  const base = useId();
  const child = Children.only(children);
  const control = isValidElement(child) ? (child as ReactElement<ControlProps>) : null;
  const id = control?.props.id ?? `${base}-control`;
  const hintId = hint ? `${base}-hint` : undefined;
  const errorId = error ? `${base}-error` : undefined;
  const describedBy =
    [control?.props["aria-describedby"], hintId, errorId].filter(Boolean).join(" ") || undefined;

  return (
    <div
      data-slot="field"
      data-invalid={error ? "" : undefined}
      className={cn("flex flex-col gap-1.5", className)}
    >
      <label htmlFor={id} className="w-fit font-sans text-sm font-medium text-ink">
        {label}
      </label>
      {control
        ? cloneElement(control, {
            id,
            "aria-describedby": describedBy,
            "aria-invalid": error ? true : control.props["aria-invalid"],
          })
        : child}
      {hint ? (
        <p id={hintId} className="font-sans text-sm text-ink-soft">
          {hint}
        </p>
      ) : null}
      {error ? (
        <p id={errorId} className="flex items-start gap-1.5 font-sans text-sm text-pen-text">
          <Icon icon={WarningCircle} size={16} className="mt-0.5" />
          {error}
        </p>
      ) : null}
    </div>
  );
}
